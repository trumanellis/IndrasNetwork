//! Pass story KDF pipeline
//!
//! Transforms a 23-slot autobiographical narrative into cryptographic keys
//! through normalization, canonical encoding, Argon2id key derivation,
//! and HKDF key expansion.

use argon2::{Argon2, Params, Version};
use hkdf::Hkdf;
use sha2::Sha512;
use unicode_normalization::UnicodeNormalization;

use crate::error::{CryptoError, CryptoResult};
use crate::pq_identity::SecureBytes;

/// Number of slots in a pass story.
pub const STORY_SLOT_COUNT: usize = 23;

/// Minimum total entropy in bits for story acceptance.
pub const MIN_ENTROPY_BITS: f64 = 256.0;

/// Argon2id parameters (higher than EncryptedKeystore's OWASP defaults).
pub const ARGON2_MEMORY_KIB: u32 = 262_144; // 256 MB
pub const ARGON2_ITERATIONS: u32 = 4;
pub const ARGON2_PARALLELISM: u32 = 4;
pub const ARGON2_OUTPUT_LEN: usize = 64; // 512-bit master key

/// Delimiter between slots in canonical encoding (null byte).
const SLOT_DELIMITER: u8 = 0x00;

/// Purpose-specific subkeys derived from the master key.
pub struct StorySubkeys {
    /// Key for identity/authentication operations (32 bytes)
    pub identity: SecureBytes,
    /// Key for encrypting data at rest (32 bytes)
    pub encryption: SecureBytes,
    /// Key for digital signatures (32 bytes)
    pub signing: SecureBytes,
    /// Key for account recovery (32 bytes)
    pub recovery: SecureBytes,
}

/// Normalize a single slot value.
///
/// Applies:
/// - Unicode NFC normalization
/// - Lowercase conversion
/// - Whitespace collapsed to single spaces
/// - Leading and trailing whitespace stripped
pub fn normalize_slot(raw: &str) -> String {
    let nfc: String = raw.nfc().collect();
    let lowered = nfc.to_lowercase();

    // Collapse whitespace and trim
    let mut result = String::with_capacity(lowered.len());
    let mut prev_was_space = true; // treat start as space to trim leading
    for ch in lowered.chars() {
        if ch.is_whitespace() {
            if !prev_was_space {
                result.push(' ');
                prev_was_space = true;
            }
        } else {
            result.push(ch);
            prev_was_space = false;
        }
    }

    // Trim trailing space
    if result.ends_with(' ') {
        result.pop();
    }

    result
}

/// Concatenate 23 normalized slots with null byte delimiter.
///
/// Returns error if any slot contains a null byte.
pub fn canonical_encode(slots: &[String; STORY_SLOT_COUNT]) -> CryptoResult<Vec<u8>> {
    // Check for null bytes in any slot
    for (i, slot) in slots.iter().enumerate() {
        if slot.as_bytes().contains(&SLOT_DELIMITER) {
            return Err(CryptoError::NullByteInSlot);
        }
        if slot.is_empty() {
            return Err(CryptoError::InvalidStory(format!(
                "Slot {} is empty",
                i + 1
            )));
        }
    }

    // Estimate capacity: sum of slot lengths + 22 delimiters
    let total_len: usize = slots.iter().map(|s| s.len()).sum::<usize>() + STORY_SLOT_COUNT - 1;
    let mut canonical = Vec::with_capacity(total_len);

    for (i, slot) in slots.iter().enumerate() {
        if i > 0 {
            canonical.push(SLOT_DELIMITER);
        }
        canonical.extend_from_slice(slot.as_bytes());
    }

    Ok(canonical)
}

/// Derive a 512-bit master key from the canonical story encoding.
///
/// Uses Argon2id with high memory parameters (256MB, 4 iterations, parallelism 4).
/// Salt should be `user_id || registration_timestamp`.
pub fn derive_master_key(canonical: &[u8], salt: &[u8]) -> CryptoResult<SecureBytes> {
    let params = Params::new(
        ARGON2_MEMORY_KIB,
        ARGON2_ITERATIONS,
        ARGON2_PARALLELISM,
        Some(ARGON2_OUTPUT_LEN),
    )
    .map_err(|e| CryptoError::KeyDerivationFailed(format!("Invalid Argon2 params: {}", e)))?;

    let argon2 = Argon2::new(argon2::Algorithm::Argon2id, Version::V0x13, params);

    let mut output = vec![0u8; ARGON2_OUTPUT_LEN];
    argon2
        .hash_password_into(canonical, salt, &mut output)
        .map_err(|e| CryptoError::KeyDerivationFailed(format!("Argon2id failed: {}", e)))?;

    Ok(SecureBytes::new(output))
}

/// Expand master key into 4 purpose-specific subkeys via HKDF-SHA512.
///
/// Each subkey is 32 bytes, derived with a unique info string.
pub fn expand_subkeys(master_key: &SecureBytes) -> CryptoResult<StorySubkeys> {
    let hk = Hkdf::<Sha512>::new(None, master_key.as_slice());

    let mut identity = vec![0u8; 32];
    let mut encryption = vec![0u8; 32];
    let mut signing = vec![0u8; 32];
    let mut recovery = vec![0u8; 32];

    hk.expand(b"indras-pass-story-identity", &mut identity)
        .map_err(|e| CryptoError::KeyDerivationFailed(format!("HKDF expand failed: {}", e)))?;
    hk.expand(b"indras-pass-story-encryption", &mut encryption)
        .map_err(|e| CryptoError::KeyDerivationFailed(format!("HKDF expand failed: {}", e)))?;
    hk.expand(b"indras-pass-story-signing", &mut signing)
        .map_err(|e| CryptoError::KeyDerivationFailed(format!("HKDF expand failed: {}", e)))?;
    hk.expand(b"indras-pass-story-recovery", &mut recovery)
        .map_err(|e| CryptoError::KeyDerivationFailed(format!("HKDF expand failed: {}", e)))?;

    Ok(StorySubkeys {
        identity: SecureBytes::new(identity),
        encryption: SecureBytes::new(encryption),
        signing: SecureBytes::new(signing),
        recovery: SecureBytes::new(recovery),
    })
}

/// Full pipeline: normalize → encode → derive → expand.
///
/// Convenience function that runs the entire KDF pipeline.
pub fn derive_keys_from_story(
    raw_slots: &[&str; STORY_SLOT_COUNT],
    salt: &[u8],
) -> CryptoResult<StorySubkeys> {
    // Normalize all slots
    let normalized: Vec<String> = raw_slots.iter().map(|s| normalize_slot(s)).collect();

    // Convert to fixed-size array
    let slots: [String; STORY_SLOT_COUNT] = normalized
        .try_into()
        .map_err(|_| CryptoError::InvalidStory("Failed to collect slots".to_string()))?;

    // Canonical encode
    let canonical = canonical_encode(&slots)?;

    // Derive master key
    let master_key = derive_master_key(&canonical, salt)?;

    // Expand to subkeys
    expand_subkeys(&master_key)
}

/// Generate a verification token from a master key.
///
/// Uses BLAKE3 to hash the master key. The token is stored server-side
/// to verify authentication without storing the story itself.
pub fn story_verification_token(master_key: &SecureBytes) -> [u8; 32] {
    *blake3::hash(master_key.as_slice()).as_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_slot_basic() {
        assert_eq!(normalize_slot("Hello"), "hello");
        assert_eq!(normalize_slot("  HELLO  "), "hello");
        assert_eq!(normalize_slot("hello   world"), "hello world");
        assert_eq!(normalize_slot("  hello   world  "), "hello world");
    }

    #[test]
    fn test_normalize_slot_unicode() {
        // NFC normalization: é as combining should equal precomposed
        assert_eq!(normalize_slot("café"), normalize_slot("café"));
        assert_eq!(normalize_slot("AMARANTH"), normalize_slot("amaranth"));
    }

    #[test]
    fn test_normalize_idempotent() {
        let inputs = ["Hello World", "  café  ", "AMARANTH", "hello   world  test"];
        for input in &inputs {
            let once = normalize_slot(input);
            let twice = normalize_slot(&once);
            assert_eq!(once, twice, "Normalization not idempotent for: {}", input);
        }
    }

    #[test]
    fn test_canonical_encode_deterministic() {
        let slots = core::array::from_fn::<String, STORY_SLOT_COUNT, _>(|i| {
            format!("word{}", i)
        });
        let enc1 = canonical_encode(&slots).unwrap();
        let enc2 = canonical_encode(&slots).unwrap();
        assert_eq!(enc1, enc2);
    }

    #[test]
    fn test_canonical_encode_null_byte_rejected() {
        let mut slots = core::array::from_fn::<String, STORY_SLOT_COUNT, _>(|i| {
            format!("word{}", i)
        });
        slots[5] = "hello\x00world".to_string();
        let result = canonical_encode(&slots);
        assert!(matches!(result, Err(CryptoError::NullByteInSlot)));
    }

    #[test]
    fn test_canonical_encode_empty_slot_rejected() {
        let mut slots = core::array::from_fn::<String, STORY_SLOT_COUNT, _>(|i| {
            format!("word{}", i)
        });
        slots[0] = String::new();
        let result = canonical_encode(&slots);
        assert!(matches!(result, Err(CryptoError::InvalidStory(_))));
    }

    #[test]
    fn test_canonical_encode_contains_delimiters() {
        let slots = core::array::from_fn::<String, STORY_SLOT_COUNT, _>(|i| {
            format!("word{}", i)
        });
        let encoded = canonical_encode(&slots).unwrap();
        // Should have exactly 22 null byte delimiters
        let delimiter_count = encoded.iter().filter(|&&b| b == 0x00).count();
        assert_eq!(delimiter_count, STORY_SLOT_COUNT - 1);
    }

    #[test]
    fn test_derive_master_key_length() {
        let canonical = b"test\x00data\x00more";
        let salt = b"user123_timestamp";
        let key = derive_master_key(canonical, salt).unwrap();
        assert_eq!(key.len(), ARGON2_OUTPUT_LEN);
    }

    #[test]
    fn test_derive_master_key_deterministic() {
        let canonical = b"test\x00data\x00more";
        let salt = b"user123_timestamp";
        let key1 = derive_master_key(canonical, salt).unwrap();
        let key2 = derive_master_key(canonical, salt).unwrap();
        assert_eq!(key1.as_slice(), key2.as_slice());
    }

    #[test]
    fn test_derive_master_key_different_input() {
        let salt = b"user123_timestamp";
        let key1 = derive_master_key(b"story_a", salt).unwrap();
        let key2 = derive_master_key(b"story_b", salt).unwrap();
        assert_ne!(key1.as_slice(), key2.as_slice());
    }

    #[test]
    fn test_expand_subkeys() {
        let master = SecureBytes::new(vec![42u8; 64]);
        let subkeys = expand_subkeys(&master).unwrap();

        // Each subkey should be 32 bytes
        assert_eq!(subkeys.identity.len(), 32);
        assert_eq!(subkeys.encryption.len(), 32);
        assert_eq!(subkeys.signing.len(), 32);
        assert_eq!(subkeys.recovery.len(), 32);

        // All subkeys should be distinct
        assert_ne!(subkeys.identity.as_slice(), subkeys.encryption.as_slice());
        assert_ne!(subkeys.identity.as_slice(), subkeys.signing.as_slice());
        assert_ne!(subkeys.identity.as_slice(), subkeys.recovery.as_slice());
        assert_ne!(subkeys.encryption.as_slice(), subkeys.signing.as_slice());
        assert_ne!(subkeys.encryption.as_slice(), subkeys.recovery.as_slice());
        assert_ne!(subkeys.signing.as_slice(), subkeys.recovery.as_slice());
    }

    #[test]
    fn test_verification_token_deterministic() {
        let master = SecureBytes::new(vec![42u8; 64]);
        let token1 = story_verification_token(&master);
        let token2 = story_verification_token(&master);
        assert_eq!(token1, token2);
    }

    #[test]
    fn test_verification_token_different_keys() {
        let master1 = SecureBytes::new(vec![42u8; 64]);
        let master2 = SecureBytes::new(vec![43u8; 64]);
        let token1 = story_verification_token(&master1);
        let token2 = story_verification_token(&master2);
        assert_ne!(token1, token2);
    }

    #[test]
    fn test_full_pipeline() {
        let raw_slots: [&str; STORY_SLOT_COUNT] = [
            "static", "collector", "autumn", "clarity",
            "vertigo", "pride", "kitchen", "amaranth",
            "librarian", "telescope", "patience",
            "compass", "silence", "cassiterite", "granite",
            "mercury", "labyrinth", "chrysalis", "horologist",
            "amaranth", "cartographer", "wanderer", "lighthouse",
        ];

        let salt = b"user_zephyr_2025";
        let subkeys = derive_keys_from_story(&raw_slots, salt).unwrap();

        assert_eq!(subkeys.identity.len(), 32);
        assert_eq!(subkeys.encryption.len(), 32);
        assert_eq!(subkeys.signing.len(), 32);
        assert_eq!(subkeys.recovery.len(), 32);
    }

    #[test]
    fn test_normalization_equivalence_in_pipeline() {
        let raw1: [&str; STORY_SLOT_COUNT] = [
            "AMARANTH", "Horologist", "autumn", "clarity",
            "vertigo", "pride", "kitchen", "static",
            "librarian", "telescope", "patience",
            "compass", "silence", "cassiterite", "granite",
            "mercury", "labyrinth", "chrysalis", "wanderer",
            "lighthouse", "cartographer", "collector", "pyrrhic",
        ];
        let raw2: [&str; STORY_SLOT_COUNT] = [
            "amaranth", "horologist", "autumn", "clarity",
            "vertigo", "pride", "kitchen", "static",
            "librarian", "telescope", "patience",
            "compass", "silence", "cassiterite", "granite",
            "mercury", "labyrinth", "chrysalis", "wanderer",
            "lighthouse", "cartographer", "collector", "pyrrhic",
        ];

        let salt = b"test_salt";
        let keys1 = derive_keys_from_story(&raw1, salt).unwrap();
        let keys2 = derive_keys_from_story(&raw2, salt).unwrap();

        assert_eq!(keys1.identity.as_slice(), keys2.identity.as_slice());
    }
}
