//! Wire format for AppFlowy updates sent over IndrasNetwork
//!
//! Each message on the wire is an `AppFlowyEnvelope` serialized with postcard.
//! The magic bytes allow filtering non-AppFlowy messages from the event stream.

use serde::{Deserialize, Serialize};

use crate::error::BridgeError;

/// Magic bytes identifying an AppFlowy bridge envelope.
/// ASCII for "AFYB" (AppFlowy Yrs Bridge).
pub const ENVELOPE_MAGIC: [u8; 4] = [0x41, 0x46, 0x59, 0x42];

/// Wire envelope wrapping a raw Yrs update for transport over IndrasNetwork.
///
/// Layout (postcard-serialized):
/// ```text
/// [magic: 4 bytes] [object_id_hash: 32 bytes] [update: variable]
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppFlowyEnvelope {
    /// Magic bytes for identification
    pub magic: [u8; 4],
    /// BLAKE3 hash of the originating object_id (for routing without revealing the ID)
    pub object_id_hash: [u8; 32],
    /// Raw Yrs update bytes (v1 encoding)
    pub update: Vec<u8>,
}

impl AppFlowyEnvelope {
    /// Create a new envelope for the given object and update.
    pub fn new(object_id_hash: [u8; 32], update: Vec<u8>) -> Self {
        Self {
            magic: ENVELOPE_MAGIC,
            object_id_hash,
            update,
        }
    }

    /// Hash an object_id to the 32-byte hash used in envelopes.
    pub fn hash_object_id(object_id: &str) -> [u8; 32] {
        *blake3::hash(object_id.as_bytes()).as_bytes()
    }

    /// Serialize this envelope to bytes.
    pub fn to_bytes(&self) -> Result<Vec<u8>, BridgeError> {
        postcard::to_allocvec(self).map_err(|e| BridgeError::Envelope(e.to_string()))
    }

    /// Deserialize an envelope from bytes.
    ///
    /// Returns `None` if the magic bytes don't match (not an AppFlowy envelope).
    pub fn from_bytes(bytes: &[u8]) -> Option<Result<Self, BridgeError>> {
        // Quick check: postcard puts the magic first, but we need to actually
        // deserialize to verify since postcard is a variable-length format.
        // However, we can do a fast pre-check by trying to deserialize and
        // then checking the magic.
        let result: Result<Self, _> = postcard::from_bytes(bytes);
        match result {
            Ok(envelope) if envelope.magic == ENVELOPE_MAGIC => Some(Ok(envelope)),
            Ok(_) => None, // Valid postcard but wrong magic
            Err(e) => {
                // Could be a non-AppFlowy message (different format entirely)
                // or a corrupt envelope. We treat both as "not ours".
                if bytes.len() >= 4 && bytes[0..4] == ENVELOPE_MAGIC {
                    // Starts with our magic but failed to parse — that's an error
                    Some(Err(BridgeError::Envelope(e.to_string())))
                } else {
                    None
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_round_trip() {
        let object_hash = AppFlowyEnvelope::hash_object_id("doc-123");
        let update = vec![1, 2, 3, 4, 5];
        let envelope = AppFlowyEnvelope::new(object_hash, update.clone());

        let bytes = envelope.to_bytes().unwrap();
        let decoded = AppFlowyEnvelope::from_bytes(&bytes)
            .expect("should be Some")
            .expect("should be Ok");

        assert_eq!(decoded.magic, ENVELOPE_MAGIC);
        assert_eq!(decoded.object_id_hash, object_hash);
        assert_eq!(decoded.update, update);
    }

    #[test]
    fn test_magic_filtering() {
        // Random bytes that don't match our format
        let garbage = vec![0x00, 0x01, 0x02, 0x03, 0x04, 0x05];
        assert!(
            AppFlowyEnvelope::from_bytes(&garbage).is_none(),
            "non-AppFlowy data should return None"
        );
    }

    #[test]
    fn test_empty_update() {
        let object_hash = AppFlowyEnvelope::hash_object_id("empty-doc");
        let envelope = AppFlowyEnvelope::new(object_hash, vec![]);

        let bytes = envelope.to_bytes().unwrap();
        let decoded = AppFlowyEnvelope::from_bytes(&bytes)
            .expect("should be Some")
            .expect("should be Ok");

        assert!(decoded.update.is_empty());
    }

    #[test]
    fn test_hash_object_id_deterministic() {
        let h1 = AppFlowyEnvelope::hash_object_id("my-doc");
        let h2 = AppFlowyEnvelope::hash_object_id("my-doc");
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_hash_object_id_different_inputs() {
        let h1 = AppFlowyEnvelope::hash_object_id("doc-a");
        let h2 = AppFlowyEnvelope::hash_object_id("doc-b");
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_large_update() {
        let object_hash = [0xABu8; 32];
        let update = vec![0xFFu8; 65536]; // 64 KiB update
        let envelope = AppFlowyEnvelope::new(object_hash, update.clone());

        let bytes = envelope.to_bytes().unwrap();
        let decoded = AppFlowyEnvelope::from_bytes(&bytes)
            .expect("should be Some")
            .expect("should be Ok");

        assert_eq!(decoded.update.len(), 65536);
        assert_eq!(decoded.update, update);
    }
}
