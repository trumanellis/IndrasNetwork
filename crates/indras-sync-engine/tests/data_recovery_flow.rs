//! Crypto-level E2E for the Plan-C personal-data backup flow.
//!
//! Mirrors `tests/account_root_recovery.rs` but exercises the file
//! shard pipeline end-to-end:
//!
//! 1. User picks a wrapping key W (in production: same W that
//!    gates the Plan-B AccountRoot envelope).
//! 2. User publishes several files; each file goes through
//!    `prepare_file_shards` — per-file ChaCha20 key + erasure
//!    coding + envelope under W.
//! 3. Some Backup Peers go offline (simulated by dropping shards).
//! 4. New device (which just reassembled W through the steward
//!    flow) pulls the remaining shards, reconstructs each file,
//!    and byte-compares against the original.
//!
//! This validates the primitives under realistic loss patterns
//! without needing real iroh transport — the network-layer E2E
//! still wants the DM-realm harness flagged in A.8/B.9.

use indras_sync_engine::file_shard::{prepare_file_shards, reconstruct_file, FileShard};

fn wrapping_key() -> [u8; 32] {
    let mut k = [0u8; 32];
    for (i, b) in k.iter_mut().enumerate() {
        *b = (i as u8).wrapping_mul(31).wrapping_add(7);
    }
    k
}

fn publish_file(contents: &[u8], label: &str, w: &[u8; 32]) -> Vec<FileShard> {
    let file_id = *blake3::hash(contents).as_bytes();
    let prepared = prepare_file_shards(contents, &file_id, label, w, 3, 5, 1_700_000_000)
        .expect("prepare file shards");
    assert_eq!(prepared.shards.len(), 5);
    prepared.shards
}

#[test]
fn recovers_multiple_files_with_parity_budget_losses() {
    let w = wrapping_key();

    let file_a = b"# Hello world\n\nfirst document".to_vec();
    let file_b = vec![0u8; 4_000]; // an empty-ish binary file
    let file_c = b"the quick brown fox jumps over the lazy dog".repeat(40);

    // "User" publishes three files. Each file ends up as 5 shards.
    let shards_a = publish_file(&file_a, "a.md", &w);
    let shards_b = publish_file(&file_b, "b.bin", &w);
    let shards_c = publish_file(&file_c, "c.md", &w);

    // Simulate that two Backup Peers are offline — lose shard_index
    // 1 and 4 across every file.
    let drop = |shards: Vec<FileShard>| -> Vec<Option<FileShard>> {
        shards
            .into_iter()
            .enumerate()
            .map(|(i, s)| if i == 1 || i == 4 { None } else { Some(s) })
            .collect()
    };

    // New device reassembles each file using only the 3 surviving
    // shards.
    let back_a = reconstruct_file(drop(shards_a), &w).unwrap();
    let back_b = reconstruct_file(drop(shards_b), &w).unwrap();
    let back_c = reconstruct_file(drop(shards_c), &w).unwrap();

    assert_eq!(back_a, file_a);
    assert_eq!(back_b, file_b);
    assert_eq!(back_c, file_c);
}

#[test]
fn losing_one_more_than_budget_fails() {
    let w = wrapping_key();
    let file = b"doomed".to_vec();
    let shards = publish_file(&file, "doom", &w);

    // Drop 3 out of 5 — below the data_threshold of 3.
    let too_few: Vec<Option<FileShard>> = shards
        .into_iter()
        .enumerate()
        .map(|(i, s)| if i < 3 { None } else { Some(s) })
        .collect();
    let err = reconstruct_file(too_few, &w).unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("at least") || msg.contains("enough"), "{msg}");
}

#[test]
fn recovery_requires_exact_wrapping_key() {
    let w_real = wrapping_key();
    let mut w_fake = w_real;
    w_fake[0] ^= 0x01;

    let file = b"account data".to_vec();
    let shards = publish_file(&file, "acct", &w_real);
    let all: Vec<Option<FileShard>> = shards.into_iter().map(Some).collect();

    // Correct W decrypts.
    let back = reconstruct_file(all.clone(), &w_real).unwrap();
    assert_eq!(back, file);

    // A single flipped bit in W — decryption fails.
    let err = reconstruct_file(all, &w_fake).unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("decrypt") || msg.contains("Crypto"), "{msg}");
}
