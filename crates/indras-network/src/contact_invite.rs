//! Contact invite codes for sharing contact links.
//!
//! Provides a human-shareable format for contact invitations,
//! following the same pattern as realm `InviteCode`.

use crate::error::{IndraError, Result};
use crate::member::MemberId;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// The URI scheme prefix for contact invite codes.
const CONTACT_INVITE_PREFIX: &str = "syncengine:contact:";

/// Inner serializable payload for a contact invite.
#[derive(Clone, Serialize, Deserialize)]
struct ContactInviteInner {
    member_id: MemberId,
    display_name: Option<String>,
}

/// A human-shareable invite code for adding a contact.
///
/// Contact invite codes can be shared as text or links.
///
/// # Format
///
/// ```text
/// syncengine:contact:<base64-encoded-payload>
/// ```
///
/// # Example
///
/// ```ignore
/// let code = network.contact_invite_code();
/// println!("Share this link: {}", code);
/// // => syncengine:contact:7xK9mN2pQ...
///
/// // Accept using the invite
/// network.accept_contact_invite(&code).await?;
/// ```
#[derive(Clone)]
pub struct ContactInviteCode {
    inner: ContactInviteInner,
}

impl ContactInviteCode {
    /// Create a new contact invite code.
    pub fn new(member_id: MemberId, display_name: Option<String>) -> Self {
        Self {
            inner: ContactInviteInner {
                member_id,
                display_name,
            },
        }
    }

    /// Parse a contact invite code from a string.
    ///
    /// Accepts both the full URI format (`syncengine:contact:...`) and
    /// raw base64-encoded payloads.
    pub fn parse(s: &str) -> Result<Self> {
        let s = s.trim();

        // Strip the prefix if present
        let base64_part = if let Some(stripped) = s.strip_prefix(CONTACT_INVITE_PREFIX) {
            stripped
        } else if s.starts_with("syncengine:") {
            return Err(IndraError::InvalidInvite {
                reason: "Unknown invite type (expected 'contact')".to_string(),
            });
        } else {
            s
        };

        // Decode base64
        let bytes = URL_SAFE_NO_PAD.decode(base64_part)?;

        // Deserialize the payload
        let inner: ContactInviteInner =
            postcard::from_bytes(&bytes).map_err(|e| IndraError::InvalidInvite {
                reason: format!("Invalid contact invite data: {}", e),
            })?;

        Ok(Self { inner })
    }

    /// Get the member ID from this invite.
    pub fn member_id(&self) -> MemberId {
        self.inner.member_id
    }

    /// Get the display name from this invite, if any.
    pub fn display_name(&self) -> Option<&str> {
        self.inner.display_name.as_deref()
    }

    /// Convert to a shareable string in URI format.
    pub fn to_uri(&self) -> String {
        format!("{}{}", CONTACT_INVITE_PREFIX, self.to_base64())
    }

    /// Convert to raw base64-encoded format.
    pub fn to_base64(&self) -> String {
        let bytes = postcard::to_allocvec(&self.inner).expect("serialization should not fail");
        URL_SAFE_NO_PAD.encode(&bytes)
    }
}

impl fmt::Debug for ContactInviteCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ContactInviteCode")
            .field(
                "member_id",
                &hex::encode(&self.inner.member_id[..8]),
            )
            .finish()
    }
}

impl fmt::Display for ContactInviteCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_uri())
    }
}

impl FromStr for ContactInviteCode {
    type Err = IndraError;

    fn from_str(s: &str) -> Result<Self> {
        Self::parse(s)
    }
}

// Simple hex encoding for debug output
mod hex {
    pub fn encode(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{:02x}", b)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_round_trip() {
        let member_id = [42u8; 32];
        let code = ContactInviteCode::new(member_id, Some("Zephyr".to_string()));

        let uri = code.to_uri();
        assert!(uri.starts_with("syncengine:contact:"));

        let parsed = ContactInviteCode::parse(&uri).unwrap();
        assert_eq!(parsed.member_id(), member_id);
        assert_eq!(parsed.display_name(), Some("Zephyr"));
    }

    #[test]
    fn test_round_trip_no_name() {
        let member_id = [7u8; 32];
        let code = ContactInviteCode::new(member_id, None);

        let uri = code.to_uri();
        let parsed = ContactInviteCode::parse(&uri).unwrap();
        assert_eq!(parsed.member_id(), member_id);
        assert_eq!(parsed.display_name(), None);
    }

    #[test]
    fn test_parse_raw_base64() {
        let member_id = [99u8; 32];
        let code = ContactInviteCode::new(member_id, Some("Nova".to_string()));

        let base64 = code.to_base64();
        let parsed = ContactInviteCode::parse(&base64).unwrap();
        assert_eq!(parsed.member_id(), member_id);
        assert_eq!(parsed.display_name(), Some("Nova"));
    }

    #[test]
    fn test_parse_invalid_prefix() {
        assert!(ContactInviteCode::parse("syncengine:realm:invalid").is_err());
    }

    #[test]
    fn test_parse_empty() {
        assert!(ContactInviteCode::parse("").is_err());
    }

    #[test]
    fn test_display_is_uri() {
        let code = ContactInviteCode::new([1u8; 32], None);
        assert_eq!(format!("{}", code), code.to_uri());
    }

    #[test]
    fn test_from_str() {
        let code = ContactInviteCode::new([5u8; 32], Some("Sage".to_string()));
        let uri = code.to_uri();
        let parsed: ContactInviteCode = uri.parse().unwrap();
        assert_eq!(parsed.member_id(), [5u8; 32]);
    }
}
