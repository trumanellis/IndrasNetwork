//! Compact identity codes using bech32 encoding.
//!
//! Provides human-friendly identity codes for sharing MemberIds.
//! Uses bech32 encoding with the `indra` human-readable part (HRP),
//! producing codes like `indra1qw508d6q...` (~58 chars).
//!
//! ## Size Comparison
//!
//! | Format | Size | QR Code Version |
//! |--------|------|-----------------|
//! | Legacy ContactInviteCode | ~500+ chars | Version 10+ (large) |
//! | Identity code (bech32) | ~58 chars | Version 3 (small) |
//! | Identity URI with name | ~75 chars | Version 4 (small) |

use crate::error::{IndraError, Result};
use crate::member::MemberId;

use bech32::{Bech32m, Hrp};
use std::fmt;
use std::str::FromStr;

/// Human-readable part for Indras identity codes.
const INDRA_HRP: &str = "indra";

/// A compact identity code for a member.
///
/// Encodes a 32-byte MemberId into a ~58-character bech32m string
/// with built-in error detection.
///
/// # Format
///
/// ```text
/// indra1<bech32m-encoded-member-id>
/// ```
///
/// # Example
///
/// ```ignore
/// let code = IdentityCode::from_member_id(my_id);
/// println!("Share this: {}", code);  // indra1qw508d6q...
///
/// let parsed = IdentityCode::parse("indra1qw508d6q...")?;
/// let member_id = parsed.member_id();
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentityCode {
    member_id: MemberId,
}

impl IdentityCode {
    /// Create an identity code from a MemberId.
    pub fn from_member_id(member_id: MemberId) -> Self {
        Self { member_id }
    }

    /// Get the MemberId from this code.
    pub fn member_id(&self) -> MemberId {
        self.member_id
    }

    /// Encode as a bech32m string.
    pub fn encode(&self) -> String {
        let hrp = Hrp::parse(INDRA_HRP).expect("valid HRP");
        bech32::encode::<Bech32m>(hrp, &self.member_id).expect("valid encoding")
    }

    /// Parse from a bech32m string.
    pub fn parse(s: &str) -> Result<Self> {
        let s = s.trim();

        // Strip optional URI prefix
        let bech32_part = if let Some(stripped) = s.strip_prefix("indra://") {
            stripped
        } else {
            s
        };

        let (hrp, data) = bech32::decode(bech32_part).map_err(|e| IndraError::InvalidInvite {
            reason: format!("Invalid identity code: {}", e),
        })?;

        if !hrp.as_str().eq_ignore_ascii_case(INDRA_HRP) {
            return Err(IndraError::InvalidInvite {
                reason: format!(
                    "Invalid identity code HRP: expected '{}', got '{}'",
                    INDRA_HRP,
                    hrp.as_str()
                ),
            });
        }

        if data.len() != 32 {
            return Err(IndraError::InvalidInvite {
                reason: format!(
                    "Invalid identity code: expected 32 bytes, got {}",
                    data.len()
                ),
            });
        }

        let mut member_id = [0u8; 32];
        member_id.copy_from_slice(&data);

        Ok(Self { member_id })
    }

    /// Create a URI with optional display name query parameter.
    ///
    /// Format: `indra1...?name=Zephyr`
    pub fn to_uri(&self, display_name: Option<&str>) -> String {
        let encoded = self.encode();
        match display_name {
            Some(name) => format!("{}?name={}", encoded, name),
            None => encoded,
        }
    }

    /// Parse a URI that may include a name query parameter.
    ///
    /// Returns (IdentityCode, Option<display_name>).
    pub fn parse_uri(s: &str) -> Result<(Self, Option<String>)> {
        let s = s.trim();

        // Split on '?' to extract query params
        let (code_part, query_part) = match s.split_once('?') {
            Some((code, query)) => (code, Some(query)),
            None => (s, None),
        };

        let code = Self::parse(code_part)?;

        let display_name = query_part.and_then(|q| {
            q.split('&')
                .find_map(|param| {
                    param.strip_prefix("name=").map(|v| v.to_string())
                })
        });

        Ok((code, display_name))
    }
}

impl fmt::Display for IdentityCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.encode())
    }
}

impl FromStr for IdentityCode {
    type Err = IndraError;

    fn from_str(s: &str) -> Result<Self> {
        Self::parse(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zephyr_id() -> MemberId {
        [1u8; 32]
    }

    fn nova_id() -> MemberId {
        [2u8; 32]
    }

    #[test]
    fn test_encode_decode_roundtrip() {
        let code = IdentityCode::from_member_id(zephyr_id());
        let encoded = code.encode();

        assert!(encoded.starts_with("indra1"));
        assert!(encoded.len() < 70, "Should be compact: {} chars", encoded.len());

        let decoded = IdentityCode::parse(&encoded).unwrap();
        assert_eq!(decoded.member_id(), zephyr_id());
    }

    #[test]
    fn test_different_ids_different_codes() {
        let code1 = IdentityCode::from_member_id(zephyr_id());
        let code2 = IdentityCode::from_member_id(nova_id());
        assert_ne!(code1.encode(), code2.encode());
    }

    #[test]
    fn test_display_is_encode() {
        let code = IdentityCode::from_member_id(zephyr_id());
        assert_eq!(format!("{}", code), code.encode());
    }

    #[test]
    fn test_from_str() {
        let code = IdentityCode::from_member_id(nova_id());
        let encoded = code.encode();
        let parsed: IdentityCode = encoded.parse().unwrap();
        assert_eq!(parsed.member_id(), nova_id());
    }

    #[test]
    fn test_uri_with_name() {
        let code = IdentityCode::from_member_id(zephyr_id());
        let uri = code.to_uri(Some("Zephyr"));

        assert!(uri.contains("?name=Zephyr"));

        let (parsed, name) = IdentityCode::parse_uri(&uri).unwrap();
        assert_eq!(parsed.member_id(), zephyr_id());
        assert_eq!(name, Some("Zephyr".to_string()));
    }

    #[test]
    fn test_uri_without_name() {
        let code = IdentityCode::from_member_id(nova_id());
        let uri = code.to_uri(None);

        let (parsed, name) = IdentityCode::parse_uri(&uri).unwrap();
        assert_eq!(parsed.member_id(), nova_id());
        assert_eq!(name, None);
    }

    #[test]
    fn test_parse_invalid() {
        assert!(IdentityCode::parse("").is_err());
        assert!(IdentityCode::parse("not-bech32").is_err());
        assert!(IdentityCode::parse("bc1qw508d6q").is_err()); // wrong HRP
    }

    #[test]
    fn test_case_insensitive() {
        let code = IdentityCode::from_member_id(zephyr_id());
        let encoded = code.encode();
        let upper = encoded.to_uppercase();

        // bech32 is case-insensitive
        let parsed = IdentityCode::parse(&upper).unwrap();
        assert_eq!(parsed.member_id(), zephyr_id());
    }

    #[test]
    fn test_whitespace_trimmed() {
        let code = IdentityCode::from_member_id(zephyr_id());
        let encoded = format!("  {}  \n", code.encode());
        let parsed = IdentityCode::parse(&encoded).unwrap();
        assert_eq!(parsed.member_id(), zephyr_id());
    }

    #[test]
    fn test_code_length() {
        // bech32m encoding of 32 bytes should be ~58 chars
        let code = IdentityCode::from_member_id(zephyr_id());
        let encoded = code.encode();
        // indra1 (6 chars) + data (~52 chars) + checksum (6 chars) ≈ 64 chars max
        assert!(encoded.len() <= 70, "Got {} chars: {}", encoded.len(), encoded);
        assert!(encoded.len() >= 50, "Got {} chars: {}", encoded.len(), encoded);
    }
}
