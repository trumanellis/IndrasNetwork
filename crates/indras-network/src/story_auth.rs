//! High-level story authentication flow.
//!
//! Orchestrates account creation, authentication, story rotation,
//! and partial recovery using the pass story system.

use std::path::{Path, PathBuf};

use indras_crypto::pass_story::{
    derive_master_key, expand_subkeys, story_verification_token,
};
use indras_crypto::entropy;
use indras_crypto::story_template::PassStory;
use indras_crypto::SecureBytes;
use indras_node::StoryKeystore;

use crate::error::{IndraError, Result};
use crate::rehearsal::RehearsalState;

/// Filename for rehearsal state persistence.
const REHEARSAL_STATE_FILENAME: &str = "rehearsal.json";

/// Result of an authentication attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthResult {
    /// Authentication succeeded.
    Success,
    /// Authentication failed (wrong story).
    Failed,
    /// Authentication succeeded, but a rehearsal is due.
    RehearsalDue,
}

/// High-level story authentication manager.
///
/// Combines StoryKeystore with rehearsal state and entropy checking.
pub struct StoryAuth {
    /// The underlying story keystore.
    keystore: StoryKeystore,
    /// Rehearsal state for drift mitigation.
    rehearsal: RehearsalState,
    /// Data directory path.
    data_dir: PathBuf,
    /// Salt used for key derivation.
    salt: Vec<u8>,
}

impl StoryAuth {
    /// Full account creation flow.
    ///
    /// 1. Validates the story passes the entropy gate
    /// 2. Derives cryptographic keys from the story
    /// 3. Initializes the keystore with generated PQ keys
    /// 4. Sets up rehearsal schedule
    pub fn create_account(
        data_dir: &Path,
        story: &PassStory,
        user_id: &[u8],
        timestamp: u64,
    ) -> Result<Self> {
        // Check entropy gate
        entropy::entropy_gate(story.slots())
            .map_err(|e| IndraError::StoryAuth {
                reason: format!("{}", e),
            })?;

        // Build salt: user_id || timestamp
        let mut salt = Vec::with_capacity(user_id.len() + 8);
        salt.extend_from_slice(user_id);
        salt.extend_from_slice(&timestamp.to_le_bytes());

        // Derive keys from story
        let canonical = story.canonical().map_err(|e| IndraError::StoryAuth {
            reason: format!("Canonical encoding failed: {}", e),
        })?;

        let master_key = derive_master_key(&canonical, &salt)
            .map_err(|e| IndraError::StoryAuth {
                reason: format!("Key derivation failed: {}", e),
            })?;

        let subkeys = expand_subkeys(&master_key)
            .map_err(|e| IndraError::StoryAuth {
                reason: format!("Key expansion failed: {}", e),
            })?;

        let token = story_verification_token(&master_key);

        // Extract 32-byte encryption key
        let encryption_key: [u8; 32] = subkeys
            .encryption
            .as_slice()
            .try_into()
            .map_err(|_| IndraError::StoryAuth {
                reason: "Invalid encryption key length".to_string(),
            })?;

        // Initialize keystore
        let mut keystore = StoryKeystore::new(data_dir);
        keystore
            .initialize(&encryption_key, token, &salt)
            .map_err(|e| IndraError::StoryAuth {
                reason: format!("Keystore initialization failed: {}", e),
            })?;

        // Set up rehearsal
        let rehearsal = RehearsalState::new();
        let data_dir = data_dir.to_path_buf();

        let auth = Self {
            keystore,
            rehearsal,
            data_dir: data_dir.clone(),
            salt,
        };

        // Persist rehearsal state
        auth.save_rehearsal_state()?;

        Ok(auth)
    }

    /// Full authentication flow.
    ///
    /// 1. Re-derives keys from the provided story
    /// 2. Verifies the token matches
    /// 3. Unlocks the keystore
    /// 4. Checks if rehearsal is due
    pub fn authenticate(
        data_dir: &Path,
        story: &PassStory,
    ) -> Result<(Self, AuthResult)> {
        let mut keystore = StoryKeystore::new(data_dir);

        if !keystore.is_initialized() {
            return Err(IndraError::StoryAuth {
                reason: "No story keystore found — create an account first".to_string(),
            });
        }

        // Load salt
        let salt = keystore.load_story_salt().map_err(|e| IndraError::StoryAuth {
            reason: format!("Failed to load salt: {}", e),
        })?;

        // Derive keys
        let canonical = story.canonical().map_err(|e| IndraError::StoryAuth {
            reason: format!("Canonical encoding failed: {}", e),
        })?;

        let master_key = derive_master_key(&canonical, &salt)
            .map_err(|e| IndraError::StoryAuth {
                reason: format!("Key derivation failed: {}", e),
            })?;

        let subkeys = expand_subkeys(&master_key)
            .map_err(|e| IndraError::StoryAuth {
                reason: format!("Key expansion failed: {}", e),
            })?;

        let token = story_verification_token(&master_key);

        let encryption_key: [u8; 32] = subkeys
            .encryption
            .as_slice()
            .try_into()
            .map_err(|_| IndraError::StoryAuth {
                reason: "Invalid encryption key length".to_string(),
            })?;

        // Authenticate
        match keystore.authenticate(&encryption_key, token) {
            Ok(()) => {}
            Err(_) => {
                return Ok((
                    Self {
                        keystore,
                        rehearsal: Self::load_or_default_rehearsal(data_dir),
                        data_dir: data_dir.to_path_buf(),
                        salt,
                    },
                    AuthResult::Failed,
                ));
            }
        }

        // Load rehearsal state
        let mut rehearsal = Self::load_or_default_rehearsal(data_dir);

        // Determine result
        let result = if rehearsal.is_due() {
            rehearsal.record_success();
            AuthResult::RehearsalDue
        } else {
            AuthResult::Success
        };

        let auth = Self {
            keystore,
            rehearsal,
            data_dir: data_dir.to_path_buf(),
            salt,
        };

        auth.save_rehearsal_state()?;

        Ok((auth, result))
    }

    /// Partial recovery: identify which stages failed.
    ///
    /// Compares the attempted story's canonical encoding against the stored
    /// verification token to determine which stages don't match.
    ///
    /// NOTE: This requires the recovery key, which stewards hold shares of.
    /// This method only provides slot-level hints, not the actual answers.
    pub fn recovery_hint(
        &self,
        attempted_story: &PassStory,
        _recovery_key: &SecureBytes,
    ) -> Result<Vec<usize>> {
        // Since the KDF is all-or-nothing, we can't actually determine
        // which individual slots are wrong without the correct story.
        //
        // For Phase 1, recovery hints work at the stage level:
        // We store a per-stage hash during creation and compare.
        //
        // For now, return empty — full implementation requires
        // per-stage commitment scheme (Phase 2).
        let _ = attempted_story;
        Ok(Vec::new())
    }

    /// Rotate to a new story.
    ///
    /// Must be currently authenticated.
    pub fn rotate(
        &mut self,
        new_story: &PassStory,
        user_id: &[u8],
        timestamp: u64,
    ) -> Result<()> {
        // Check entropy gate on new story
        entropy::entropy_gate(new_story.slots())
            .map_err(|e| IndraError::StoryAuth {
                reason: format!("{}", e),
            })?;

        // Build new salt
        let mut new_salt = Vec::with_capacity(user_id.len() + 8);
        new_salt.extend_from_slice(user_id);
        new_salt.extend_from_slice(&timestamp.to_le_bytes());

        // Derive new keys
        let canonical = new_story.canonical().map_err(|e| IndraError::StoryAuth {
            reason: format!("Canonical encoding failed: {}", e),
        })?;

        let master_key = derive_master_key(&canonical, &new_salt)
            .map_err(|e| IndraError::StoryAuth {
                reason: format!("Key derivation failed: {}", e),
            })?;

        let subkeys = expand_subkeys(&master_key)
            .map_err(|e| IndraError::StoryAuth {
                reason: format!("Key expansion failed: {}", e),
            })?;

        let new_token = story_verification_token(&master_key);

        let new_encryption_key: [u8; 32] = subkeys
            .encryption
            .as_slice()
            .try_into()
            .map_err(|_| IndraError::StoryAuth {
                reason: "Invalid encryption key length".to_string(),
            })?;

        // Rotate keystore
        self.keystore
            .rotate_story(&new_encryption_key, new_token, &new_salt)
            .map_err(|e| IndraError::StoryAuth {
                reason: format!("Key rotation failed: {}", e),
            })?;

        // Reset rehearsal for new story
        self.rehearsal = RehearsalState::new();
        self.salt = new_salt;
        self.save_rehearsal_state()?;

        Ok(())
    }

    /// Get the rendered story for confirmation display.
    pub fn render_story(story: &PassStory) -> String {
        story.render()
    }

    /// Get the rehearsal state.
    pub fn rehearsal(&self) -> &RehearsalState {
        &self.rehearsal
    }

    /// Access the underlying keystore.
    pub fn keystore(&self) -> &StoryKeystore {
        &self.keystore
    }

    /// Lock the keystore.
    pub fn lock(&mut self) {
        self.keystore.lock();
    }

    // ========== Private Helpers ==========

    fn rehearsal_path(data_dir: &Path) -> PathBuf {
        data_dir.join(REHEARSAL_STATE_FILENAME)
    }

    fn save_rehearsal_state(&self) -> Result<()> {
        let bytes = self.rehearsal.to_bytes().map_err(|e| IndraError::StoryAuth {
            reason: format!("Failed to serialize rehearsal state: {}", e),
        })?;

        std::fs::write(Self::rehearsal_path(&self.data_dir), bytes)
            .map_err(|e| IndraError::StoryAuth {
                reason: format!("Failed to write rehearsal state: {}", e),
            })?;

        Ok(())
    }

    fn load_or_default_rehearsal(data_dir: &Path) -> RehearsalState {
        let path = Self::rehearsal_path(data_dir);
        if path.exists() {
            if let Ok(bytes) = std::fs::read(&path) {
                if let Ok(state) = RehearsalState::from_bytes(&bytes) {
                    return state;
                }
            }
        }
        RehearsalState::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indras_crypto::story_template::PassStory;
    use tempfile::TempDir;

    fn test_raw_slots() -> [&'static str; 23] {
        [
            "cassiterite", "pyrrhic", "amaranth", "horologist",
            "vermicelli", "cumulonimbus", "astrolabe", "cartographer",
            "chrysalis", "stalactite", "phosphorescence",
            "fibonacci", "tessellation", "calligraphy", "obsidian",
            "quicksilver", "labyrinthine", "bioluminescence", "synesthesia",
            "perihelion", "soliloquy", "archipelago", "phantasmagoria",
        ]
    }

    #[test]
    fn test_create_and_authenticate() {
        let temp_dir = TempDir::new().unwrap();
        let raw = test_raw_slots();
        let story = PassStory::from_raw(&raw).unwrap();

        // Create account
        let _auth = StoryAuth::create_account(
            temp_dir.path(),
            &story,
            b"user_zephyr",
            1234567890,
        )
        .unwrap();

        // Authenticate with same story
        let (_auth, result) = StoryAuth::authenticate(
            temp_dir.path(),
            &story,
        )
        .unwrap();

        // Should succeed (might be RehearsalDue since it was just created)
        assert!(
            result == AuthResult::Success || result == AuthResult::RehearsalDue,
            "Expected success or rehearsal due, got {:?}",
            result
        );
    }

    #[test]
    fn test_wrong_story_fails() {
        let temp_dir = TempDir::new().unwrap();
        let raw = test_raw_slots();
        let story = PassStory::from_raw(&raw).unwrap();

        // Create account
        let _auth = StoryAuth::create_account(
            temp_dir.path(),
            &story,
            b"user_zephyr",
            1234567890,
        )
        .unwrap();

        // Try different story
        let wrong_raw: [&str; 23] = [
            "wrong", "words", "here", "completely",
            "different", "story", "from", "the",
            "original", "one", "that",
            "was", "used", "to", "create",
            "the", "account", "in", "the",
            "first", "place", "cassiterite", "pyrrhic",
        ];
        let wrong_story = PassStory::from_raw(&wrong_raw).unwrap();

        let (_, result) = StoryAuth::authenticate(
            temp_dir.path(),
            &wrong_story,
        )
        .unwrap();

        assert_eq!(result, AuthResult::Failed);
    }

    #[test]
    fn test_normalization_equivalence() {
        let temp_dir = TempDir::new().unwrap();
        let raw = test_raw_slots();
        let story = PassStory::from_raw(&raw).unwrap();

        // Create with lowercase
        let _auth = StoryAuth::create_account(
            temp_dir.path(),
            &story,
            b"user_zephyr",
            1234567890,
        )
        .unwrap();

        // Authenticate with uppercase versions
        let upper_raw: [&str; 23] = [
            "CASSITERITE", "PYRRHIC", "AMARANTH", "HOROLOGIST",
            "VERMICELLI", "CUMULONIMBUS", "ASTROLABE", "CARTOGRAPHER",
            "CHRYSALIS", "STALACTITE", "PHOSPHORESCENCE",
            "FIBONACCI", "TESSELLATION", "CALLIGRAPHY", "OBSIDIAN",
            "QUICKSILVER", "LABYRINTHINE", "BIOLUMINESCENCE", "SYNESTHESIA",
            "PERIHELION", "SOLILOQUY", "ARCHIPELAGO", "PHANTASMAGORIA",
        ];
        let upper_story = PassStory::from_raw(&upper_raw).unwrap();

        let (_, result) = StoryAuth::authenticate(
            temp_dir.path(),
            &upper_story,
        )
        .unwrap();

        assert!(
            result == AuthResult::Success || result == AuthResult::RehearsalDue,
            "Case-insensitive auth should succeed, got {:?}",
            result
        );
    }
}
