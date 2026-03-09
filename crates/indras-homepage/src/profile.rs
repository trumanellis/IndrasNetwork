//! Profile data types for the homepage

use serde::{Deserialize, Serialize};

/// Profile information displayed on the homepage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    /// Display name shown on the page
    pub display_name: String,
    /// Username (used in URL path)
    pub username: String,
    /// Optional bio/description
    pub bio: Option<String>,
    /// iroh node public key (hex-encoded, for verification)
    pub public_key: String,
}

impl Profile {
    /// Create a new profile with required fields
    pub fn new(display_name: impl Into<String>, username: impl Into<String>, public_key: impl Into<String>) -> Self {
        Self {
            display_name: display_name.into(),
            username: username.into(),
            bio: None,
            public_key: public_key.into(),
        }
    }

    /// Set the bio
    pub fn with_bio(mut self, bio: impl Into<String>) -> Self {
        self.bio = Some(bio.into());
        self
    }
}
