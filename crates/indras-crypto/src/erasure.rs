//! Reed-Solomon erasure coding for personal-data backup.
//!
//! Distributes an arbitrary-length payload across `data_shards +
//! parity_shards` equal-size shards such that any `data_shards` of
//! them reconstruct the original. Plan-C uses this to spread
//! encrypted file bytes across the user's Backup Peers — device
//! loss becomes a re-fetch, not a permanent write-off.
//!
//! The underlying `reed-solomon-erasure` crate operates on the
//! GF(2^8) field so every byte is a symbol. Shards are padded to
//! a common length; the original payload length is carried out-of-
//! band so decode can strip trailing zeros.

use reed_solomon_erasure::galois_8::ReedSolomon;

/// Errors produced by the erasure-coding primitives.
#[derive(Debug, thiserror::Error)]
pub enum ErasureError {
    #[error("data_shards must be at least 1")]
    ZeroDataShards,
    #[error("parity_shards must be at least 1")]
    ZeroParityShards,
    #[error("total shards must be <= 255")]
    TooManyShards,
    #[error("underlying reed-solomon error: {0}")]
    ReedSolomon(String),
    #[error("need at least {data_shards} shards to reconstruct, got {present}")]
    NotEnoughShards { data_shards: usize, present: usize },
    #[error("shard length mismatch: shard {index} is {got}, expected {want}")]
    ShardLengthMismatch {
        index: usize,
        got: usize,
        want: usize,
    },
    #[error("original_len ({original}) exceeds data region capacity ({capacity})")]
    OriginalLengthTooLarge { original: usize, capacity: usize },
}

impl From<reed_solomon_erasure::Error> for ErasureError {
    fn from(value: reed_solomon_erasure::Error) -> Self {
        ErasureError::ReedSolomon(format!("{value}"))
    }
}

/// Shape of a successful encode — carries the per-shard bytes plus
/// the metadata the decoder needs to strip trailing padding.
#[derive(Debug, Clone)]
pub struct ErasureShards {
    /// All shards, first `data_shards` of which are pure data and
    /// the rest are parity. Every entry has the same length.
    pub shards: Vec<Vec<u8>>,
    /// Length of the original payload in bytes.
    pub original_len: usize,
    /// Number of data shards (reconstruction threshold).
    pub data_shards: usize,
    /// Number of parity shards (how many can be lost).
    pub parity_shards: usize,
}

/// Encode `data` into `data_shards + parity_shards` equal-size
/// shards. Any `data_shards` of them will reconstruct the
/// original via [`decode`].
pub fn encode(
    data: &[u8],
    data_shards: usize,
    parity_shards: usize,
) -> Result<ErasureShards, ErasureError> {
    if data_shards == 0 {
        return Err(ErasureError::ZeroDataShards);
    }
    if parity_shards == 0 {
        return Err(ErasureError::ZeroParityShards);
    }
    if data_shards + parity_shards > 255 {
        return Err(ErasureError::TooManyShards);
    }

    // Pad the input so it divides evenly into `data_shards` pieces.
    let shard_len = (data.len() + data_shards - 1).max(data_shards) / data_shards;
    let padded_len = shard_len * data_shards;
    let mut padded = data.to_vec();
    padded.resize(padded_len, 0);

    let mut shards: Vec<Vec<u8>> = Vec::with_capacity(data_shards + parity_shards);
    for i in 0..data_shards {
        shards.push(padded[i * shard_len..(i + 1) * shard_len].to_vec());
    }
    for _ in 0..parity_shards {
        shards.push(vec![0u8; shard_len]);
    }

    let rs = ReedSolomon::new(data_shards, parity_shards)?;
    rs.encode(&mut shards)?;

    Ok(ErasureShards {
        shards,
        original_len: data.len(),
        data_shards,
        parity_shards,
    })
}

/// Reconstruct the original payload from any `data_shards` intact
/// survivors. Missing shards should be supplied as `None` and
/// intact ones as `Some(bytes)`; every supplied shard must match
/// the common shard length.
pub fn decode(
    shards: &[Option<Vec<u8>>],
    data_shards: usize,
    parity_shards: usize,
    original_len: usize,
) -> Result<Vec<u8>, ErasureError> {
    if data_shards == 0 {
        return Err(ErasureError::ZeroDataShards);
    }
    if parity_shards == 0 {
        return Err(ErasureError::ZeroParityShards);
    }
    let expected_total = data_shards + parity_shards;
    if shards.len() != expected_total {
        return Err(ErasureError::NotEnoughShards {
            data_shards,
            present: shards.iter().filter(|s| s.is_some()).count(),
        });
    }

    let present = shards.iter().filter(|s| s.is_some()).count();
    if present < data_shards {
        return Err(ErasureError::NotEnoughShards {
            data_shards,
            present,
        });
    }

    // All supplied shards must be the same length.
    let shard_len = shards
        .iter()
        .find_map(|s| s.as_ref().map(|v| v.len()))
        .unwrap_or(0);
    for (i, s) in shards.iter().enumerate() {
        if let Some(v) = s {
            if v.len() != shard_len {
                return Err(ErasureError::ShardLengthMismatch {
                    index: i,
                    got: v.len(),
                    want: shard_len,
                });
            }
        }
    }
    let capacity = shard_len * data_shards;
    if original_len > capacity {
        return Err(ErasureError::OriginalLengthTooLarge {
            original: original_len,
            capacity,
        });
    }

    let mut work: Vec<Option<Vec<u8>>> = shards.to_vec();
    let rs = ReedSolomon::new(data_shards, parity_shards)?;
    rs.reconstruct(&mut work)?;

    let mut out = Vec::with_capacity(capacity);
    for slot in work.iter().take(data_shards) {
        out.extend_from_slice(slot.as_ref().expect("reconstruct fills data shards"));
    }
    out.truncate(original_len);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_payload(n: usize) -> Vec<u8> {
        (0..n).map(|i| (i as u8).wrapping_mul(17).wrapping_add(3)).collect()
    }

    #[test]
    fn encode_decode_roundtrip_all_shards_present() {
        let payload = sample_payload(1024);
        let encoded = encode(&payload, 4, 2).unwrap();
        assert_eq!(encoded.shards.len(), 6);
        let first_len = encoded.shards[0].len();
        assert!(encoded.shards.iter().all(|s| s.len() == first_len));

        let shards: Vec<Option<Vec<u8>>> = encoded.shards.iter().cloned().map(Some).collect();
        let recovered = decode(&shards, 4, 2, encoded.original_len).unwrap();
        assert_eq!(recovered, payload);
    }

    #[test]
    fn survives_loss_of_parity_count_shards() {
        let payload = sample_payload(2048);
        let encoded = encode(&payload, 3, 2).unwrap();
        let mut shards: Vec<Option<Vec<u8>>> =
            encoded.shards.iter().cloned().map(Some).collect();
        // Drop two shards — one data, one parity — which equals
        // parity_shards total; must still reconstruct.
        shards[0] = None;
        shards[4] = None;
        let recovered = decode(&shards, 3, 2, encoded.original_len).unwrap();
        assert_eq!(recovered, payload);
    }

    #[test]
    fn below_threshold_fails() {
        let payload = sample_payload(500);
        let encoded = encode(&payload, 3, 2).unwrap();
        let mut shards: Vec<Option<Vec<u8>>> =
            encoded.shards.iter().cloned().map(Some).collect();
        // Drop 3 shards — beyond the parity budget of 2.
        shards[0] = None;
        shards[1] = None;
        shards[2] = None;
        let err = decode(&shards, 3, 2, encoded.original_len).unwrap_err();
        assert!(matches!(err, ErasureError::NotEnoughShards { .. }));
    }

    #[test]
    fn small_payload_handles_padding() {
        let payload = b"hi".to_vec();
        let encoded = encode(&payload, 2, 1).unwrap();
        let shards: Vec<Option<Vec<u8>>> =
            encoded.shards.iter().cloned().map(Some).collect();
        let recovered = decode(&shards, 2, 1, encoded.original_len).unwrap();
        assert_eq!(recovered, payload);
    }

    #[test]
    fn empty_payload_roundtrips_as_empty() {
        let payload: Vec<u8> = Vec::new();
        let encoded = encode(&payload, 2, 1).unwrap();
        let shards: Vec<Option<Vec<u8>>> =
            encoded.shards.iter().cloned().map(Some).collect();
        let recovered = decode(&shards, 2, 1, encoded.original_len).unwrap();
        assert!(recovered.is_empty());
    }

    #[test]
    fn rejects_invalid_parameters() {
        let payload = sample_payload(10);
        assert!(matches!(encode(&payload, 0, 2), Err(ErasureError::ZeroDataShards)));
        assert!(matches!(encode(&payload, 2, 0), Err(ErasureError::ZeroParityShards)));
        assert!(matches!(encode(&payload, 200, 200), Err(ErasureError::TooManyShards)));
    }
}
