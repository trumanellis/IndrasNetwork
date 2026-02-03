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
    /// Inbox interface ID for connection requests.
    #[serde(default)]
    inbox_id: Option<[u8; 32]>,
    /// Serialized EndpointAddr for P2P bootstrap.
    #[serde(default)]
    bootstrap: Option<Vec<u8>>,
    /// Interface encryption key for the inbox.
    #[serde(default)]
    inbox_key: Option<[u8; 32]>,
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
    /// Maximum display name length in a contact invite.
    const MAX_DISPLAY_NAME_LEN: usize = 64;

    /// Create a new contact invite code.
    ///
    /// Display names are sanitized: control characters are removed and
    /// the length is capped at 64 characters.
    pub fn new(member_id: MemberId, display_name: Option<String>) -> Self {
        let display_name = display_name.map(|n| {
            n.chars()
                .take(Self::MAX_DISPLAY_NAME_LEN)
                .filter(|c| !c.is_control())
                .collect::<String>()
        });
        Self {
            inner: ContactInviteInner {
                member_id,
                display_name,
                inbox_id: None,
                bootstrap: None,
                inbox_key: None,
            },
        }
    }

    /// Attach inbox transport info for P2P bootstrap.
    pub fn with_inbox(
        mut self,
        inbox_id: [u8; 32],
        bootstrap: Vec<u8>,
        inbox_key: [u8; 32],
    ) -> Self {
        self.inner.inbox_id = Some(inbox_id);
        self.inner.bootstrap = Some(bootstrap);
        self.inner.inbox_key = Some(inbox_key);
        self
    }

    /// Get the inbox interface ID, if present.
    pub fn inbox_id(&self) -> Option<&[u8; 32]> {
        self.inner.inbox_id.as_ref()
    }

    /// Get the bootstrap address bytes, if present.
    pub fn bootstrap(&self) -> Option<&[u8]> {
        self.inner.bootstrap.as_deref()
    }

    /// Get the inbox interface encryption key, if present.
    pub fn inbox_key(&self) -> Option<&[u8; 32]> {
        self.inner.inbox_key.as_ref()
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

    #[test]
    fn test_display_name_truncated_at_64_chars() {
        let long_name = "A".repeat(100);
        let code = ContactInviteCode::new([1u8; 32], Some(long_name));
        assert_eq!(code.display_name().unwrap().len(), 64);
    }

    #[test]
    fn test_display_name_control_chars_removed() {
        let name_with_controls = "Zephyr\x00\x07\nOrion".to_string();
        let code = ContactInviteCode::new([2u8; 32], Some(name_with_controls));
        assert_eq!(code.display_name(), Some("ZephyrOrion"));
    }

    #[test]
    fn test_display_name_empty_after_sanitization() {
        let only_controls = "\x00\x01\x02\x03".to_string();
        let code = ContactInviteCode::new([3u8; 32], Some(only_controls));
        assert_eq!(code.display_name(), Some(""));
    }
}
