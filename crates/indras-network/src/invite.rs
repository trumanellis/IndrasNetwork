//! Invite codes for joining realms.
//!
//! Provides a human-shareable format for realm invitations.

use crate::error::{IndraError, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use indras_core::InterfaceId;
use indras_node::InviteKey;
use std::fmt;
use std::str::FromStr;

/// The URI scheme prefix for invite codes.
const INVITE_PREFIX: &str = "indra:realm:";

/// A human-shareable invite code for joining a realm.
///
/// Invite codes can be shared as text, QR codes, or links.
///
/// # Format
///
/// ```text
/// indra:realm:<base64-encoded-invite-key>
/// ```
///
/// # Example
///
/// ```ignore
/// let invite = realm.invite_code();
/// println!("Share this invite: {}", invite);
/// // => indra:realm:7xK9mN2pQ...
///
/// // Join using the invite
/// let realm = network.join(invite).await?;
/// ```
#[derive(Clone)]
pub struct InviteCode {
    inner: InviteKey,
}

impl InviteCode {
    /// Create a new invite code from an invite key.
    pub fn new(key: InviteKey) -> Self {
        Self { inner: key }
    }

    /// Parse an invite code from a string.
    ///
    /// Accepts both the full URI format (`indra:realm:...`) and
    /// raw base64-encoded invite keys.
    pub fn parse(s: &str) -> Result<Self> {
        let s = s.trim();

        // Strip the prefix if present
        let base64_part = if let Some(stripped) = s.strip_prefix(INVITE_PREFIX) {
            stripped
        } else if s.starts_with("indra:") {
            return Err(IndraError::InvalidInvite {
                reason: "Unknown invite type (expected 'realm')".to_string(),
            });
        } else {
            s
        };

        // Decode base64
        let bytes = URL_SAFE_NO_PAD.decode(base64_part)?;

        // Deserialize the invite key
        let key: InviteKey = postcard::from_bytes(&bytes).map_err(|e| IndraError::InvalidInvite {
            reason: format!("Invalid invite data: {}", e),
        })?;

        Ok(Self { inner: key })
    }

    /// Get the realm ID this invite is for.
    pub fn realm_id(&self) -> InterfaceId {
        self.inner.interface_id
    }

    /// Convert to a shareable string in URI format.
    pub fn to_uri(&self) -> String {
        format!("{}{}", INVITE_PREFIX, self.to_base64())
    }

    /// Convert to raw base64-encoded format.
    pub fn to_base64(&self) -> String {
        let bytes = postcard::to_allocvec(&self.inner).expect("serialization should not fail");
        URL_SAFE_NO_PAD.encode(&bytes)
    }

    /// Generate a QR code image for this invite.
    #[cfg(feature = "qr")]
    pub fn to_qr(&self) -> Result<image::DynamicImage> {
        use qrcode::QrCode;

        let code = QrCode::new(self.to_uri().as_bytes()).map_err(|e| IndraError::Artifact(format!("Failed to generate QR code: {}", e)))?;

        let image = code.render::<image::Luma<u8>>().build();
        Ok(image::DynamicImage::ImageLuma8(image))
    }

    /// Save a QR code to a PNG file.
    #[cfg(feature = "qr")]
    pub fn save_qr_png(&self, path: impl AsRef<std::path::Path>) -> Result<()> {
        let img = self.to_qr()?;
        img.save(path).map_err(|e| IndraError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("Failed to save QR code: {}", e),
        )))
    }

    // ============================================================
    // Escape hatch
    // ============================================================

    /// Access the underlying invite key.
    pub fn invite_key(&self) -> &InviteKey {
        &self.inner
    }

    /// Consume and return the underlying invite key.
    pub fn into_invite_key(self) -> InviteKey {
        self.inner
    }
}

impl fmt::Debug for InviteCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InviteCode")
            .field("realm_id", &hex::encode(&self.inner.interface_id.as_bytes()[..8]))
            .finish()
    }
}

impl fmt::Display for InviteCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_uri())
    }
}

impl FromStr for InviteCode {
    type Err = IndraError;

    fn from_str(s: &str) -> Result<Self> {
        Self::parse(s)
    }
}

impl From<InviteKey> for InviteCode {
    fn from(key: InviteKey) -> Self {
        Self::new(key)
    }
}

// For ergonomic API - allow passing string directly to join()
impl<'a> From<&'a str> for InviteCodeRef<'a> {
    fn from(s: &'a str) -> Self {
        InviteCodeRef(s)
    }
}

impl From<InviteCode> for InviteCodeRef<'static> {
    fn from(_: InviteCode) -> Self {
        // This is a marker type, actual conversion happens elsewhere
        panic!("Use InviteCode directly")
    }
}

/// Helper type for accepting either InviteCode or &str in join()
#[allow(dead_code)] // Reserved for ergonomic join() API
pub struct InviteCodeRef<'a>(pub(crate) &'a str);

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
    fn test_invite_parse_invalid() {
        assert!(InviteCode::parse("indra:foo:invalid").is_err());
        assert!(InviteCode::parse("").is_err());
    }

    #[test]
    fn test_invite_prefix() {
        assert!(INVITE_PREFIX.starts_with("indra:"));
    }
}
