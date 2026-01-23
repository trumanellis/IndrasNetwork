//! Keystore for identity persistence
//!
//! Handles loading and saving the node's secret key to disk,
//! ensuring identity persists across node restarts.

use std::path::{Path, PathBuf};

use iroh::SecretKey;
use tracing::{debug, info};

use crate::error::{NodeError, NodeResult};

/// Filename for the secret key
const KEY_FILENAME: &str = "identity.key";

/// Keystore for managing node identity persistence
///
/// The keystore saves and loads the node's secret key from disk,
/// allowing the node to maintain the same identity across restarts.
pub struct Keystore {
    /// Path to the keystore directory
    path: PathBuf,
}

impl Keystore {
    /// Create a new keystore with the given data directory
    pub fn new(data_dir: &Path) -> Self {
        Self {
            path: data_dir.to_path_buf(),
        }
    }

    /// Get the path to the key file
    fn key_path(&self) -> PathBuf {
        self.path.join(KEY_FILENAME)
    }

    /// Load existing key or generate a new one
    ///
    /// If a key file exists, loads and returns it.
    /// Otherwise, generates a new random key, saves it, and returns it.
    pub fn load_or_generate(&self) -> NodeResult<SecretKey> {
        let key_path = self.key_path();

        if key_path.exists() {
            self.load()
        } else {
            info!("No existing identity found, generating new key");
            let key = SecretKey::generate(&mut rand::rng());
            self.save(&key)?;
            Ok(key)
        }
    }

    /// Load an existing key from disk
    pub fn load(&self) -> NodeResult<SecretKey> {
        let key_path = self.key_path();

        let bytes = std::fs::read(&key_path)
            .map_err(|e| NodeError::Keystore(format!("Failed to read key file: {}", e)))?;

        if bytes.len() != 32 {
            return Err(NodeError::Keystore(format!(
                "Invalid key file: expected 32 bytes, got {}",
                bytes.len()
            )));
        }

        let mut key_bytes = [0u8; 32];
        key_bytes.copy_from_slice(&bytes);

        let key = SecretKey::from_bytes(&key_bytes);
        debug!(
            identity = %key.public().fmt_short(),
            "Loaded identity from keystore"
        );

        Ok(key)
    }

    /// Save a key to disk
    pub fn save(&self, key: &SecretKey) -> NodeResult<()> {
        // Ensure directory exists
        std::fs::create_dir_all(&self.path)
            .map_err(|e| NodeError::Keystore(format!("Failed to create keystore dir: {}", e)))?;

        let key_path = self.key_path();

        // Write key bytes
        std::fs::write(&key_path, key.to_bytes())
            .map_err(|e| NodeError::Keystore(format!("Failed to write key file: {}", e)))?;

        // Set restrictive permissions on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            std::fs::set_permissions(&key_path, perms)
                .map_err(|e| NodeError::Keystore(format!("Failed to set key permissions: {}", e)))?;
        }

        info!(
            identity = %key.public().fmt_short(),
            path = %key_path.display(),
            "Saved identity to keystore"
        );

        Ok(())
    }

    /// Check if a key file exists
    pub fn exists(&self) -> bool {
        self.key_path().exists()
    }

    /// Delete the key file (use with caution!)
    pub fn delete(&self) -> NodeResult<()> {
        let key_path = self.key_path();
        if key_path.exists() {
            std::fs::remove_file(&key_path)
                .map_err(|e| NodeError::Keystore(format!("Failed to delete key file: {}", e)))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_generate_and_load() {
        let temp_dir = TempDir::new().unwrap();
        let keystore = Keystore::new(temp_dir.path());

        // Should not exist initially
        assert!(!keystore.exists());

        // Generate new key
        let key1 = keystore.load_or_generate().unwrap();
        assert!(keystore.exists());

        // Load same key
        let key2 = keystore.load_or_generate().unwrap();
        assert_eq!(key1.public(), key2.public());

        // Explicit load should also work
        let key3 = keystore.load().unwrap();
        assert_eq!(key1.public(), key3.public());
    }

    #[test]
    fn test_save_and_load() {
        let temp_dir = TempDir::new().unwrap();
        let keystore = Keystore::new(temp_dir.path());

        let original_key = SecretKey::generate(&mut rand::rng());
        keystore.save(&original_key).unwrap();

        let loaded_key = keystore.load().unwrap();
        assert_eq!(original_key.public(), loaded_key.public());
    }

    #[test]
    fn test_delete() {
        let temp_dir = TempDir::new().unwrap();
        let keystore = Keystore::new(temp_dir.path());

        keystore.load_or_generate().unwrap();
        assert!(keystore.exists());

        keystore.delete().unwrap();
        assert!(!keystore.exists());
    }

    #[test]
    fn test_invalid_key_file() {
        let temp_dir = TempDir::new().unwrap();
        let keystore = Keystore::new(temp_dir.path());

        // Write invalid data
        std::fs::create_dir_all(temp_dir.path()).unwrap();
        std::fs::write(temp_dir.path().join(KEY_FILENAME), b"too short").unwrap();

        let result = keystore.load();
        assert!(result.is_err());
    }
}
