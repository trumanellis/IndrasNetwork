//! End-to-end arc test: Plan A + Plan B + Plan C together.
//!
//! Walks the full lifecycle offline (no iroh) so every primitive
//! in the frictionless-recovery stack gets exercised inside one
//! scenario:
//!
//! 1. Creator generates `AccountRoot` → signs initial
//!    `DeviceCertificate` → seeds `DeviceRoster`.
//! 2. Creator draws wrapping key W → seals root into
//!    `AccountRootEnvelope` → Shamir-splits W across 5 stewards →
//!    each share wrapped to the steward's own ML-KEM ek.
//! 3. Creator publishes 3 files; each file is ChaCha20'd under a
//!    per-file key, the per-file key sealed under W, and the
//!    ciphertext erasure-coded into 5 `FileShard`s.
//! 4. Creator's device is lost — root sk dropped, W dropped,
//!    plaintext files dropped. Only the envelope, roster, shares,
//!    and file shards survive.
//! 5. New device bootstraps its own PQ identity + KEM keypair →
//!    asks K=3 stewards for help → stewards re-wrap their shares
//!    to the new device's ek.
//! 6. New device decrypts the 3 releases → Shamir-reassembles W →
//!    unseals the root → signs a fresh `DeviceCertificate` → the
//!    roster now trusts two devices.
//! 7. New device uses the same W → unseals each file's per-file
//!    key → reconstructs every file byte-for-byte.

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
use indras_sync_engine::file_shard::{prepare_file_shards, reconstruct_file, FileShard};

fn random_wrapping_key() -> [u8; SHAMIR_SECRET_SIZE] {
    use rand::RngCore;
    let mut k = [0u8; SHAMIR_SECRET_SIZE];
    rand::thread_rng().fill_bytes(&mut k);
    k
}

#[test]
fn full_account_recovery_including_files() {
    // ── Step 1: Account creation ──────────────────────────────
    let creator_root = AccountRoot::generate();
    let creator_device = PQIdentity::generate();
    let creator_cert = DeviceCertificate::sign(
        creator_device.verifying_key_bytes(),
        "Creator's laptop",
        1_700_000_000_000,
        &creator_root,
    );
    let mut roster = DeviceRoster {
        account_root_ref: Some(AccountRootRef::from_root(&creator_root)),
        devices: vec![creator_cert.clone()],
    };

    // ── Step 2: Seal root + split wrapping key ────────────────
    let w = random_wrapping_key();
    let envelope: AccountRootEnvelope =
        seal_account_root(&creator_root, &w, 1).expect("seal root");

    const N_STEWARDS: usize = 5;
    const K_THRESHOLD: u8 = 3;
    let steward_kps: Vec<PQKemKeyPair> =
        (0..N_STEWARDS).map(|_| PQKemKeyPair::generate()).collect();
    let steward_eks: Vec<PQEncapsulationKey> =
        steward_kps.iter().map(|kp| kp.encapsulation_key()).collect();
    let shares = split_secret(&w, K_THRESHOLD, N_STEWARDS as u8).expect("split W");
    let steward_envelopes: Vec<EncryptedStewardShare> = shares
        .iter()
        .zip(&steward_eks)
        .map(|(share, ek)| {
            encrypt_share_for_steward(share, K_THRESHOLD, 1, ek).expect("wrap for steward")
        })
        .collect();

    // ── Step 3: Publish three files under the same W ──────────
    let file_readme = b"# Welcome to my vault\n\nHello world.".to_vec();
    let file_photo = (0u8..=255).cycle().take(4_096).collect::<Vec<_>>();
    let file_notes = b"- first note\n- second note\n- third note\n".to_vec();
    let files: Vec<(Vec<u8>, &str)> = vec![
        (file_readme.clone(), "readme.md"),
        (file_photo.clone(), "photo.bin"),
        (file_notes.clone(), "notes.md"),
    ];
    let shard_sets: Vec<Vec<FileShard>> = files
        .iter()
        .map(|(bytes, label)| {
            let file_id = *blake3::hash(bytes).as_bytes();
            prepare_file_shards(bytes, &file_id, *label, &w, 3, 5, 1_700_000_000_100)
                .expect("prepare shards")
                .shards
        })
        .collect();

    // ── Step 4: Device loss ───────────────────────────────────
    // Drop everything the creator held locally.
    drop(creator_root);
    let _obliterated_w = w;
    // (plaintext `files` and `shard_sets` stand in for "what
    // survives on the network" — the envelope, roster, steward
    // envelopes, and file shards. The creator's copy is gone.)

    // ── Step 5: New device bootstraps, asks K stewards ────────
    let new_device = PQIdentity::generate();
    let new_device_kem = PQKemKeyPair::generate();
    let new_device_ek = new_device_kem.encapsulation_key();

    let releasing = [0usize, 2, 4]; // stewards who approved
    let releases: Vec<EncryptedStewardShare> = releasing
        .iter()
        .map(|&i| {
            let share = steward_envelopes[i]
                .decrypt(&steward_kps[i])
                .expect("steward decrypts own share");
            encrypt_share_for_steward(&share, K_THRESHOLD, 1, &new_device_ek)
                .expect("rewrap to new device")
        })
        .collect();

    // ── Step 6: New device assembles W, unseals root, signs ──
    let recovered_shares: Vec<_> = releases
        .iter()
        .map(|r| r.decrypt(&new_device_kem).expect("new device decrypt"))
        .collect();
    let recovered_w = combine_shares(&recovered_shares, K_THRESHOLD).expect("combine W");

    let recovered_root = unseal_account_root(&envelope, &recovered_w).expect("unseal");
    let new_cert = DeviceCertificate::sign(
        new_device.verifying_key_bytes(),
        "Recovered device",
        1_700_000_500_000,
        &recovered_root,
    );
    drop(recovered_root);

    roster.upsert(new_cert.clone());
    assert_eq!(roster.devices.len(), 2, "roster now has creator + new device");
    assert!(roster.device_is_trusted(&new_device.verifying_key_bytes()));
    assert!(roster.device_is_trusted(&creator_device.verifying_key_bytes()));

    // ── Step 7: New device reconstructs each file under the
    //        same recovered W (no loss this time — every shard is
    //        available via the network). Also validates a one-
    //        shard-per-file loss case.
    for (idx, (original, _)) in files.iter().enumerate() {
        let all: Vec<Option<FileShard>> =
            shard_sets[idx].iter().cloned().map(Some).collect();
        let recovered = reconstruct_file(all, &recovered_w).expect("reconstruct file");
        assert_eq!(&recovered, original, "file {} reconstructs byte-for-byte", idx);
    }

    // Parity-budget-loss variant on the notes file.
    let mut lossy: Vec<Option<FileShard>> =
        shard_sets[2].iter().cloned().map(Some).collect();
    lossy[1] = None;
    lossy[3] = None;
    let recovered_notes = reconstruct_file(lossy, &recovered_w).unwrap();
    assert_eq!(recovered_notes, file_notes);
}
