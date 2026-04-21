//! Encrypted-and-erasure-coded file shards distributed across
//! Backup Peers.
//!
//! Two layers of cryptography wrap the file bytes:
//!
//! 1. **Per-file key.** A fresh random 32-byte key K is drawn each
//!    time a file is published. K encrypts the file bytes under
//!    ChaCha20-Poly1305.
//! 2. **Root-wrapping-key envelope.** K itself is encrypted again
//!    under the account's Shamir-split wrapping key W (the same W
//!    the stewards collectively hold in Plan B). The ciphertext of
//!    K is embedded in every shard doc.
//!
//! After those two encryption steps, the resulting per-file
//! ciphertext goes through Reed-Solomon erasure coding into
//! `data_threshold + parity_shards` equal-size shards. Each shard
//! lives in its own [`FileShard`] CRDT doc keyed
//! [`file_shard_doc_key`] inside the sender↔peer DM realm.
//!
//! Recovery is the reverse: collect ≥ `data_threshold` shards,
//! erasure-decode the ciphertext, unwrap K with the reassembled W
//! (obtained from the steward flow), decrypt, done.
//!
//! The on-the-wire doc stays opaque to the holding peer — they
//! see only a byte blob and the wrapped per-file key, neither of
//! which is useful without W.

use chacha20poly1305::{
    aead::{Aead, KeyInit},
    ChaCha20Poly1305,
};
use serde::{Deserialize, Serialize};

use indras_crypto::erasure::{decode as erasure_decode, encode as erasure_encode, ErasureError};
use indras_network::document::DocumentSchema;

/// Key prefix for per-file per-shard CRDT docs.
pub const FILE_SHARD_KEY_PREFIX: &str = "_file_shard:";

/// Build the doc key for shard `index` of `file_id`.
pub fn file_shard_doc_key(file_id: &[u8; 32], shard_index: u8) -> String {
    format!(
        "{}{}:{}",
        FILE_SHARD_KEY_PREFIX,
        hex::encode(file_id),
        shard_index
    )
}

/// CRDT doc holding one shard of a backed-up file.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FileShard {
    /// 32-byte content-addressed id of the file's plaintext —
    /// stable across republishes of the same content.
    pub file_id: [u8; 32],
    /// `0..shard_count`.
    pub shard_index: u8,
    /// Total number of shards published (`data_threshold + parity`).
    pub shard_count: u8,
    /// Minimum shards needed to reconstruct (K of `shard_count`).
    pub data_threshold: u8,
    /// Byte length of the encrypted-file-ciphertext before erasure
    /// padding. Needed at decode time.
    pub ciphertext_len: u64,
    /// One Reed-Solomon shard of the per-file ciphertext.
    pub shard_bytes: Vec<u8>,
    /// Envelope: per-file key K encrypted under the account's
    /// wrapping key W. Same value in every shard of a given
    /// version — CRDT merge uses last-writer-wins so the envelope
    /// stays consistent across shards after a re-publish.
    pub per_file_key_envelope: Vec<u8>,
    /// 12-byte ChaCha20-Poly1305 nonce used when sealing the
    /// per-file key.
    pub per_file_key_nonce: Vec<u8>,
    /// 12-byte nonce used when encrypting the file bytes with the
    /// per-file key. Same across all shards of the file.
    pub file_nonce: Vec<u8>,
    /// Plain-language label ("photos/shot1.jpg"), shown in UI.
    pub label: String,
    /// Wall-clock millis of this shard's publication. Drives LWW
    /// merge so a re-save supersedes earlier versions.
    pub created_at_millis: i64,
}

impl DocumentSchema for FileShard {
    fn merge(&mut self, remote: Self) {
        if remote.created_at_millis > self.created_at_millis {
            *self = remote;
        }
    }
}

/// Output of [`prepare_file_shards`] — ready to publish one entry
/// per peer.
#[derive(Debug, Clone)]
pub struct PreparedShardSet {
    /// One shard per peer, in the order the caller supplied peers.
    pub shards: Vec<FileShard>,
    /// K — decode threshold.
    pub data_threshold: u8,
    /// N — total published.
    pub shard_count: u8,
}

/// Errors from the prepare / reconstruct pipeline.
#[derive(Debug, thiserror::Error)]
pub enum FileShardError {
    #[error("data_threshold must be at least 1")]
    ZeroThreshold,
    #[error("total shards must equal peer count ({peers}); got {total}")]
    ShardCountMismatch { peers: usize, total: usize },
    #[error("wrapping key must be exactly 32 bytes")]
    InvalidWrappingKey,
    #[error("erasure coding error: {0}")]
    Erasure(#[from] ErasureError),
    #[error("encryption error: {0}")]
    Crypto(String),
    #[error("shard mismatch: {0}")]
    ShardMismatch(String),
}

/// Encrypt + erasure-encode a file into `peer_count` shards.
///
/// The same `wrapping_key` that gates the `AccountRootEnvelope`
/// (Plan B) also gates the per-file key — once a user has run
/// steward recovery and reassembled W, every file becomes
/// recoverable by the same flow.
pub fn prepare_file_shards(
    file_bytes: &[u8],
    file_id: &[u8; 32],
    label: impl Into<String>,
    wrapping_key: &[u8; 32],
    data_threshold: u8,
    peer_count: u8,
    created_at_millis: i64,
) -> Result<PreparedShardSet, FileShardError> {
    if data_threshold == 0 {
        return Err(FileShardError::ZeroThreshold);
    }
    if peer_count < data_threshold {
        return Err(FileShardError::ShardCountMismatch {
            peers: peer_count as usize,
            total: data_threshold as usize,
        });
    }
    let parity = peer_count - data_threshold;

    // 1. Draw a per-file symmetric key.
    let per_file_key: [u8; 32] = {
        use rand::RngCore;
        let mut k = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut k);
        k
    };

    // 2. Encrypt file bytes with the per-file key.
    let file_cipher = ChaCha20Poly1305::new_from_slice(&per_file_key)
        .map_err(|_| FileShardError::Crypto("invalid per-file key length".into()))?;
    let file_nonce = random_nonce();
    let encrypted_file = file_cipher
        .encrypt(file_nonce.as_ref().into(), file_bytes)
        .map_err(|e| FileShardError::Crypto(format!("file encrypt: {e}")))?;
    let ciphertext_len = encrypted_file.len() as u64;

    // 3. Erasure-code the encrypted file.
    let erasure = erasure_encode(
        &encrypted_file,
        data_threshold as usize,
        parity as usize,
    )?;

    // 4. Seal the per-file key under the wrapping key.
    let wrap_cipher = ChaCha20Poly1305::new_from_slice(wrapping_key)
        .map_err(|_| FileShardError::InvalidWrappingKey)?;
    let key_nonce = random_nonce();
    let per_file_key_envelope = wrap_cipher
        .encrypt(key_nonce.as_ref().into(), per_file_key.as_slice())
        .map_err(|e| FileShardError::Crypto(format!("key seal: {e}")))?;

    // 5. Emit one FileShard per peer.
    let label = label.into();
    let shards = erasure
        .shards
        .into_iter()
        .enumerate()
        .map(|(idx, bytes)| FileShard {
            file_id: *file_id,
            shard_index: idx as u8,
            shard_count: peer_count,
            data_threshold,
            ciphertext_len,
            shard_bytes: bytes,
            per_file_key_envelope: per_file_key_envelope.clone(),
            per_file_key_nonce: key_nonce.to_vec(),
            file_nonce: file_nonce.to_vec(),
            label: label.clone(),
            created_at_millis,
        })
        .collect();

    Ok(PreparedShardSet {
        shards,
        data_threshold,
        shard_count: peer_count,
    })
}

/// Reassemble a file from any subset of shards that meets the
/// threshold.
///
/// `shards` must have exactly `shard_count` entries, with `Some`
/// for each intact survivor and `None` for missing ones. At least
/// `data_threshold` must be `Some`. Every `Some` shard must carry
/// the same envelope / nonces — otherwise callers are mixing
/// versions and decode will error out.
pub fn reconstruct_file(
    shards: Vec<Option<FileShard>>,
    wrapping_key: &[u8; 32],
) -> Result<Vec<u8>, FileShardError> {
    let first = shards
        .iter()
        .find_map(|s| s.as_ref())
        .ok_or_else(|| FileShardError::ShardMismatch("no shards provided".into()))?;
    let data_threshold = first.data_threshold;
    let shard_count = first.shard_count;
    let envelope = first.per_file_key_envelope.clone();
    let key_nonce = first.per_file_key_nonce.clone();
    let file_nonce = first.file_nonce.clone();
    let ciphertext_len = first.ciphertext_len as usize;

    if shards.len() != shard_count as usize {
        return Err(FileShardError::ShardCountMismatch {
            peers: shards.len(),
            total: shard_count as usize,
        });
    }
    for s in shards.iter().flatten() {
        if s.data_threshold != data_threshold
            || s.shard_count != shard_count
            || s.per_file_key_envelope != envelope
            || s.per_file_key_nonce != key_nonce
            || s.file_nonce != file_nonce
        {
            return Err(FileShardError::ShardMismatch(
                "shards belong to different versions".into(),
            ));
        }
    }

    // Erasure decode the encrypted file bytes.
    let erasure_shards: Vec<Option<Vec<u8>>> = shards
        .iter()
        .map(|s| s.as_ref().map(|s| s.shard_bytes.clone()))
        .collect();
    let parity = shard_count - data_threshold;
    let encrypted_file = erasure_decode(
        &erasure_shards,
        data_threshold as usize,
        parity as usize,
        ciphertext_len,
    )?;

    // Unwrap the per-file key with the account wrapping key.
    let wrap_cipher = ChaCha20Poly1305::new_from_slice(wrapping_key)
        .map_err(|_| FileShardError::InvalidWrappingKey)?;
    let per_file_key = wrap_cipher
        .decrypt(key_nonce.as_slice().into(), envelope.as_slice())
        .map_err(|_| FileShardError::Crypto("envelope decrypt failed".into()))?;
    if per_file_key.len() != 32 {
        return Err(FileShardError::Crypto(
            "unwrapped per-file key is wrong length".into(),
        ));
    }
    let mut k32 = [0u8; 32];
    k32.copy_from_slice(&per_file_key);

    // Decrypt the file bytes.
    let file_cipher = ChaCha20Poly1305::new_from_slice(&k32)
        .map_err(|_| FileShardError::Crypto("invalid per-file key length".into()))?;
    file_cipher
        .decrypt(file_nonce.as_slice().into(), encrypted_file.as_slice())
        .map_err(|_| FileShardError::Crypto("file decrypt failed — wrong key or tampered".into()))
}

fn random_nonce() -> [u8; 12] {
    use rand::RngCore;
    let mut n = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut n);
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doc_key_encodes_file_id_and_index() {
        let file_id = [0x77u8; 32];
        let key = file_shard_doc_key(&file_id, 2);
        assert!(key.starts_with(FILE_SHARD_KEY_PREFIX));
        assert!(key.ends_with(":2"));
    }

    #[test]
    fn full_roundtrip_with_all_shards() {
        let wrapping = [0x44u8; 32];
        let file = b"the quick brown fox jumps over the lazy dog".to_vec();
        let file_id = *blake3::hash(&file).as_bytes();
        let prepared =
            prepare_file_shards(&file, &file_id, "fox.txt", &wrapping, 3, 5, 100).unwrap();
        assert_eq!(prepared.shards.len(), 5);
        let all: Vec<Option<FileShard>> = prepared.shards.into_iter().map(Some).collect();
        let recovered = reconstruct_file(all, &wrapping).unwrap();
        assert_eq!(recovered, file);
    }

    #[test]
    fn survives_loss_of_parity_count_shards() {
        let wrapping = [0x55u8; 32];
        let file = vec![0u8, 1, 2, 3, 4, 5, 6, 7, 8, 9].repeat(50);
        let file_id = *blake3::hash(&file).as_bytes();
        let prepared =
            prepare_file_shards(&file, &file_id, "digits", &wrapping, 3, 5, 100).unwrap();
        let mut kept: Vec<Option<FileShard>> =
            prepared.shards.into_iter().map(Some).collect();
        kept[1] = None;
        kept[4] = None;
        let recovered = reconstruct_file(kept, &wrapping).unwrap();
        assert_eq!(recovered, file);
    }

    #[test]
    fn wrong_wrapping_key_fails_reconstruct() {
        let wrapping = [0x66u8; 32];
        let attacker = [0x77u8; 32];
        let file = b"sealed content".to_vec();
        let file_id = *blake3::hash(&file).as_bytes();
        let prepared =
            prepare_file_shards(&file, &file_id, "sealed", &wrapping, 2, 3, 100).unwrap();
        let all: Vec<Option<FileShard>> = prepared.shards.into_iter().map(Some).collect();
        let err = reconstruct_file(all, &attacker).unwrap_err();
        assert!(matches!(err, FileShardError::Crypto(_)));
    }

    #[test]
    fn version_mismatch_rejected() {
        let wrapping = [0x99u8; 32];
        let file = b"mixing versions".to_vec();
        let file_id = *blake3::hash(&file).as_bytes();
        let v1 = prepare_file_shards(&file, &file_id, "m", &wrapping, 2, 3, 100).unwrap();
        let v2 = prepare_file_shards(&file, &file_id, "m", &wrapping, 2, 3, 200).unwrap();
        let mut mixed: Vec<Option<FileShard>> = vec![
            Some(v1.shards[0].clone()),
            Some(v2.shards[1].clone()),
            Some(v1.shards[2].clone()),
        ];
        let err = reconstruct_file(mixed.clone(), &wrapping).unwrap_err();
        assert!(matches!(err, FileShardError::ShardMismatch(_)));
        // Sanity: same-version reconstruct still works.
        mixed[1] = Some(v1.shards[1].clone());
        reconstruct_file(mixed, &wrapping).unwrap();
    }

    #[test]
    fn merge_prefers_newer_version() {
        let older = FileShard {
            created_at_millis: 100,
            label: "v1".into(),
            ..FileShard::default()
        };
        let newer = FileShard {
            created_at_millis: 500,
            label: "v2".into(),
            ..FileShard::default()
        };
        let mut a = older.clone();
        a.merge(newer.clone());
        assert_eq!(a.created_at_millis, 500);
        assert_eq!(a.label, "v2");
    }
}
