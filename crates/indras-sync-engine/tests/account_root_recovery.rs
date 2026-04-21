//! Full-cycle crypto E2E for the Plan-B AccountRoot recovery loop.
//!
//! Exercises the whole pipeline offline — no real iroh transport —
//! so we can validate the primitives interoperate end-to-end:
//!
//! 1. Account creator generates `AccountRoot`.
//! 2. Creator signs the initial `DeviceCertificate`.
//! 3. Creator picks a 32-byte wrapping key, seals the root into an
//!    `AccountRootEnvelope`.
//! 4. Creator Shamir-splits the wrapping key across N stewards,
//!    encrypts each share to a steward's ML-KEM ek.
//! 5. Creator "goes offline" — their root is dropped.
//! 6. A new device generates its own PQ identity + KEM keypair.
//! 7. K stewards decrypt their shares with their KEM keypair and
//!    re-encrypt each to the new device's KEM ek.
//! 8. New device decrypts the K shares, Shamir-reassembles the
//!    wrapping key, unseals the envelope, reconstructs the root.
//! 9. New device signs a fresh `DeviceCertificate` with the
//!    recovered root, upserts into the `DeviceRoster`, and drops
//!    the root.
//! 10. A third peer verifies the new device cert against the
//!     roster — trusted without ever seeing the root sk.

use indras_crypto::account_root::{AccountRoot, AccountRootRef};
use indras_crypto::device_cert::DeviceCertificate;
use indras_crypto::pq_identity::PQIdentity;
use indras_crypto::pq_kem::{PQEncapsulationKey, PQKemKeyPair};
use indras_crypto::shamir::{combine_shares, split_secret, SHAMIR_SECRET_SIZE};
use indras_crypto::steward_share::{encrypt_share_for_steward, EncryptedStewardShare};
use indras_sync_engine::account_root_envelope::{
    seal_account_root, unseal_account_root, AccountRootEnvelope,
};
use indras_sync_engine::device_roster::DeviceRoster;

fn random_wrapping_key() -> [u8; SHAMIR_SECRET_SIZE] {
    use rand::RngCore;
    let mut k = [0u8; SHAMIR_SECRET_SIZE];
    rand::thread_rng().fill_bytes(&mut k);
    k
}

#[test]
fn full_plan_b_recovery_cycle() {
    // --- 1. Account creator generates root + initial cert ---
    let creator_root = AccountRoot::generate();
    let creator_device = PQIdentity::generate();
    let creator_cert = DeviceCertificate::sign(
        creator_device.verifying_key_bytes(),
        "Creator's laptop",
        1_700_000_000_000,
        &creator_root,
    );
    assert!(creator_cert.verify(&creator_root.verifying_key()));

    // The published roster seeds with just the creator's cert.
    let mut roster = DeviceRoster {
        account_root_ref: Some(AccountRootRef::from_root(&creator_root)),
        devices: vec![creator_cert.clone()],
    };

    // --- 2. Seal the root under a fresh wrapping key, publish env ---
    let wrapping_key = random_wrapping_key();
    let envelope = seal_account_root(&creator_root, &wrapping_key, 1).expect("seal root");
    assert!(!envelope.encrypted_sk.is_empty());

    // --- 3. Split the wrapping key across 5 stewards, K = 3 ---
    const N_STEWARDS: usize = 5;
    const K_THRESHOLD: u8 = 3;

    let steward_kps: Vec<PQKemKeyPair> =
        (0..N_STEWARDS).map(|_| PQKemKeyPair::generate()).collect();
    let steward_eks: Vec<PQEncapsulationKey> = steward_kps
        .iter()
        .map(|kp| kp.encapsulation_key())
        .collect();

    let shares = split_secret(&wrapping_key, K_THRESHOLD, N_STEWARDS as u8).expect("split");
    let steward_shares: Vec<EncryptedStewardShare> = shares
        .iter()
        .zip(&steward_eks)
        .map(|(share, ek)| {
            encrypt_share_for_steward(share, K_THRESHOLD, 1, ek).expect("encrypt share")
        })
        .collect();

    // --- 4. Creator goes offline. Drop their root + plaintext
    //        wrapping key. The envelope + shares + roster persist
    //        on the network.
    drop(creator_root);
    let _obliterated_wrapping_key = wrapping_key; // go out of scope below

    // --- 5. New device bootstraps a fresh identity ---
    let new_device = PQIdentity::generate();
    let new_device_kem = PQKemKeyPair::generate();
    let new_device_ek = new_device_kem.encapsulation_key();

    // --- 6. K stewards (say #0, #2, #4) re-wrap their shares to
    //        the new device's KEM ek ---
    let releasing = [0usize, 2, 4];
    let releases: Vec<EncryptedStewardShare> = releasing
        .iter()
        .map(|&i| {
            let share = steward_shares[i]
                .decrypt(&steward_kps[i])
                .expect("steward decrypts own share");
            encrypt_share_for_steward(&share, K_THRESHOLD, 1, &new_device_ek)
                .expect("rewrap to new device")
        })
        .collect();

    // --- 7. New device decrypts its K releases + reassembles
    //        wrapping key ---
    let recovered_shares: Vec<_> = releases
        .iter()
        .map(|r| r.decrypt(&new_device_kem).expect("new device decrypts"))
        .collect();
    let recovered_wrapping = combine_shares(&recovered_shares, K_THRESHOLD).expect("combine");

    // --- 8. New device unseals the envelope -> root -> signs cert ---
    let recovered_root = unseal_account_root(&envelope, &recovered_wrapping).expect("unseal");
    let new_cert = DeviceCertificate::sign(
        new_device.verifying_key_bytes(),
        "New device (recovered)",
        1_700_000_500_000,
        &recovered_root,
    );
    drop(recovered_root); // minimize exposure window

    // Verify the freshly-signed cert against the roster's known
    // root vk (without ever re-materializing the secret key).
    let root_vk_bytes = roster
        .account_root_ref
        .as_ref()
        .expect("roster has root ref")
        .public()
        .expect("root public rehydrates");
    assert!(new_cert.verify(&root_vk_bytes));

    // --- 9. Upsert into roster + verify via the public API ---
    roster.upsert(new_cert.clone());
    assert_eq!(roster.devices.len(), 2);
    assert!(roster.device_is_trusted(&new_device.verifying_key_bytes()));
    assert!(roster.device_is_trusted(&creator_device.verifying_key_bytes()));

    // --- 10. Forged cert signed by a DIFFERENT root is rejected ---
    let attacker_root = AccountRoot::generate();
    let attacker_device = PQIdentity::generate();
    let attacker_cert = DeviceCertificate::sign(
        attacker_device.verifying_key_bytes(),
        "Impostor",
        1_700_000_900_000,
        &attacker_root,
    );
    roster.upsert(attacker_cert.clone());
    assert!(!roster.device_is_trusted(&attacker_device.verifying_key_bytes()));
}

#[test]
fn below_threshold_cannot_unseal_envelope() {
    let root = AccountRoot::generate();
    let wrapping_key = random_wrapping_key();
    let envelope = seal_account_root(&root, &wrapping_key, 1).unwrap();

    let steward_kps: Vec<PQKemKeyPair> = (0..5).map(|_| PQKemKeyPair::generate()).collect();
    let eks: Vec<_> = steward_kps.iter().map(|kp| kp.encapsulation_key()).collect();
    let shares = split_secret(&wrapping_key, 3, 5).unwrap();
    let enc: Vec<_> = shares
        .iter()
        .zip(&eks)
        .map(|(s, ek)| encrypt_share_for_steward(s, 3, 1, ek).unwrap())
        .collect();

    // Only two stewards release.
    let subset = [0usize, 1];
    let decrypted: Vec<_> = subset
        .iter()
        .map(|&i| enc[i].decrypt(&steward_kps[i]).unwrap())
        .collect();

    // combine_shares either errors or returns a key that can't
    // unseal the envelope.
    match combine_shares(&decrypted, 3) {
        Ok(bad_key) => {
            let err = unseal_account_root(&envelope, &bad_key);
            assert!(err.is_err(), "bad wrapping key must not unseal the envelope");
        }
        Err(_) => { /* Shamir rejected the shortfall — acceptable. */ }
    }
}
