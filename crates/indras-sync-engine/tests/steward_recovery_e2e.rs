//! End-to-end steward recovery test.
//!
//! Validates the full Phase 1 loop:
//! 1. Create a `StoryAuth` account with a pass story.
//! 2. Prepare a K-of-N share split via `StoryAuth::prepare_steward_recovery`.
//! 3. Lock the keystore — simulate the user losing in-memory auth state.
//! 4. Collect K shares from stewards, decrypt each with the steward's
//!    ML-KEM-768 keypair, and reassemble the encryption subkey via
//!    `steward_recovery::recover_encryption_subkey`.
//! 5. Re-authenticate the keystore with the recovered subkey and the
//!    on-disk verification token.
//! 6. Load the at-rest PQ identity with the unlocked keystore, sign a
//!    message, and verify the signature. This proves the recovered
//!    subkey is functionally identical to the original — an independent
//!    party with K steward cooperations could restore access.

use std::path::Path;

use indras_crypto::pq_kem::PQKemKeyPair;
use indras_crypto::story_template::PassStory;
use indras_node::StoryKeystore;
use indras_sync_engine::steward_recovery::{
    recover_encryption_subkey, StewardId,
};
use indras_sync_engine::story_auth::StoryAuth;
use tempfile::TempDir;

fn test_raw_slots() -> [&'static str; 23] {
    [
        "cassiterite", "pyrrhic", "amaranth", "horologist",
        "vermicelli", "cumulonimbus", "astrolabe", "cartographer",
        "chrysalis", "stalactite", "phosphorescence",
        "fibonacci", "tessellation", "calligraphy", "obsidian",
        "quicksilver", "labyrinthine", "bioluminescence", "synesthesia",
        "perihelion", "soliloquy", "archipelago", "phantasmagoria",
    ]
}

fn read_verification_token(data_dir: &Path) -> [u8; 32] {
    let bytes = std::fs::read(data_dir.join("story.token")).expect("read token file");
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    out
}

#[test]
fn recovery_restores_keystore_unlock() {
    let temp = TempDir::new().unwrap();
    let data_dir = temp.path();

    let story = PassStory::from_raw(&test_raw_slots()).unwrap();

    // 1. Create the account. This writes encrypted PQ keys, salt, and the
    // verification token to disk.
    let _auth = StoryAuth::create_account(data_dir, &story, b"user_e2e", 1_700_000_000).unwrap();

    // Snapshot the original PQ identity's verifying key so we can compare
    // after recovery. We load it while the account is freshly authenticated.
    let original_vk_short = {
        let (mut ks, auth_result) = StoryAuth::authenticate(data_dir, &story).unwrap();
        assert!(matches!(
            auth_result,
            indras_sync_engine::story_auth::AuthResult::Success
                | indras_sync_engine::story_auth::AuthResult::RehearsalDue
        ));
        let id = ks.keystore().load_or_generate_pq_identity().unwrap();
        let short = id.verifying_key().short_id();
        ks.lock();
        short
    };

    // 2. Nominate 5 stewards and prepare the split.
    let stewards: Vec<(StewardId, PQKemKeyPair)> = (0..5)
        .map(|i| {
            (
                StewardId::new(format!("steward-{}", i).into_bytes()),
                PQKemKeyPair::generate(),
            )
        })
        .collect();
    let eks: Vec<_> = stewards
        .iter()
        .map(|(id, kp)| (id.clone(), kp.encapsulation_key()))
        .collect();

    let prepared = StoryAuth::prepare_steward_recovery(data_dir, &story, &eks, 3, 1).unwrap();
    assert_eq!(prepared.manifest.threshold, 3);
    assert_eq!(prepared.manifest.total_shares, 5);
    assert_eq!(prepared.encrypted_shares.len(), 5);

    // The manifest was persisted.
    assert!(data_dir.join("steward_recovery.json").exists());

    // 3. Simulate losing in-memory auth: drop everything referencing the
    // subkey. The on-disk state that remains is: encrypted PQ keys, salt,
    // verification token, steward manifest, and the encrypted shares held
    // by each steward (here, still in our local `prepared.encrypted_shares`
    // Vec as a stand-in for the shares having been shipped out).
    drop(prepared.manifest);

    // 4. Three stewards cooperate. Each decrypts their share with their
    // own KEM keypair.
    let releasing = [0usize, 2, 4];
    let decrypted = releasing
        .iter()
        .map(|&i| {
            prepared.encrypted_shares[i]
                .decrypt(&stewards[i].1)
                .expect("steward decrypt share")
        })
        .collect::<Vec<_>>();

    let recovered_subkey = recover_encryption_subkey(&decrypted, 3).unwrap();

    // 5. Re-authenticate a fresh keystore handle with the recovered subkey
    // and the on-disk verification token. This simulates the recovered
    // user sitting down at a device that still has the at-rest encrypted
    // PQ keys but no memory of the pass story.
    let token = read_verification_token(data_dir);
    let mut recovered_ks = StoryKeystore::new(data_dir);
    assert!(recovered_ks.is_initialized());
    recovered_ks
        .authenticate(&recovered_subkey, token)
        .expect("authenticate with recovered subkey");

    // 6. Load the PQ identity. The ChaCha20-Poly1305 decrypt will only
    // succeed if the recovered subkey is byte-identical to the original.
    let recovered_identity = recovered_ks
        .load_or_generate_pq_identity()
        .expect("load PQ identity after recovery");
    assert_eq!(
        recovered_identity.verifying_key().short_id(),
        original_vk_short,
        "recovered keystore must surface the same PQ identity"
    );

    // Sign & verify a round-trip message to close the loop.
    let msg = b"recovery loop closed";
    let sig = recovered_identity.sign(msg);
    let vk = recovered_identity.verifying_key();
    assert!(vk.verify(msg, &sig), "signature must verify under the recovered identity");
}

#[test]
fn recovery_below_threshold_cannot_unlock_keystore() {
    let temp = TempDir::new().unwrap();
    let data_dir = temp.path();

    let story = PassStory::from_raw(&test_raw_slots()).unwrap();
    let _auth = StoryAuth::create_account(data_dir, &story, b"user_below", 1_700_000_000).unwrap();

    let stewards: Vec<(StewardId, PQKemKeyPair)> = (0..5)
        .map(|i| {
            (
                StewardId::new(format!("steward-{}", i).into_bytes()),
                PQKemKeyPair::generate(),
            )
        })
        .collect();
    let eks: Vec<_> = stewards
        .iter()
        .map(|(id, kp)| (id.clone(), kp.encapsulation_key()))
        .collect();

    let prepared = StoryAuth::prepare_steward_recovery(data_dir, &story, &eks, 3, 1).unwrap();

    // Only 2 stewards cooperate — below the K=3 threshold.
    let only_two = [0usize, 1]
        .iter()
        .map(|&i| prepared.encrypted_shares[i].decrypt(&stewards[i].1).unwrap())
        .collect::<Vec<_>>();

    // Either the recover call errors (count check) or it yields a
    // pseudorandom subkey that fails to unlock the keystore.
    let token = read_verification_token(data_dir);
    let unlock_succeeded = match recover_encryption_subkey(&only_two, 3) {
        Ok(bad_subkey) => {
            let mut ks = StoryKeystore::new(data_dir);
            // With the stored token, authenticate() only checks token
            // equality — so it "succeeds". The real proof of failure
            // is that the bad subkey cannot decrypt the PQ identity.
            let _ = ks.authenticate(&bad_subkey, token);
            ks.load_or_generate_pq_identity().is_ok()
        }
        Err(_) => false,
    };
    assert!(
        !unlock_succeeded,
        "below-threshold share set must not yield a key that decrypts the at-rest PQ identity"
    );
}

#[test]
fn encrypted_shares_survive_wire_roundtrip() {
    let temp = TempDir::new().unwrap();
    let data_dir = temp.path();

    let story = PassStory::from_raw(&test_raw_slots()).unwrap();
    let _auth = StoryAuth::create_account(data_dir, &story, b"user_wire", 1_700_000_000).unwrap();

    let stewards: Vec<(StewardId, PQKemKeyPair)> = (0..5)
        .map(|i| {
            (
                StewardId::new(format!("steward-{}", i).into_bytes()),
                PQKemKeyPair::generate(),
            )
        })
        .collect();
    let eks: Vec<_> = stewards
        .iter()
        .map(|(id, kp)| (id.clone(), kp.encapsulation_key()))
        .collect();

    let prepared = StoryAuth::prepare_steward_recovery(data_dir, &story, &eks, 3, 1).unwrap();

    // Serialize every encrypted share to bytes (simulating transport) and
    // reconstruct on the other side before recovering.
    let wire: Vec<Vec<u8>> = prepared
        .encrypted_shares
        .iter()
        .map(|e| e.to_bytes().unwrap())
        .collect();

    let restored: Vec<_> = wire
        .iter()
        .map(|b| {
            indras_crypto::steward_share::EncryptedStewardShare::from_bytes(b).unwrap()
        })
        .collect();

    let decrypted = [1usize, 2, 3]
        .iter()
        .map(|&i| restored[i].decrypt(&stewards[i].1).unwrap())
        .collect::<Vec<_>>();

    let recovered_subkey = recover_encryption_subkey(&decrypted, 3).unwrap();

    let token = read_verification_token(data_dir);
    let mut ks = StoryKeystore::new(data_dir);
    ks.authenticate(&recovered_subkey, token).unwrap();
    ks.load_or_generate_pq_identity().unwrap();
}
