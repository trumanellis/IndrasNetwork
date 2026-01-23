//! Keystore for identity persistence
//!
//! Handles loading and saving the node's keys to disk,
//! ensuring identity persists across node restarts.
//!
//! Stores:
//! - Iroh Ed25519 key (for transport layer)
//! - PQ identity (ML-DSA-65 for signatures)
//! - PQ KEM key pair (ML-KEM-768 for key encapsulation)

use std::path::{Path, PathBuf};

use iroh::SecretKey;
use tracing::{debug, info};

use indras_crypto::{PQIdentity, PQKemKeyPair};

use crate::error::{NodeError, NodeResult};

/// Filename for the iroh secret key
const IROH_KEY_FILENAME: &str = "identity.key";

/// Filename for the PQ signing key (private)
const PQ_SIGNING_KEY_FILENAME: &str = "identity_sk.pq";

/// Filename for the PQ verifying key (public)
const PQ_VERIFYING_KEY_FILENAME: &str = "identity_pk.pq";

/// Filename for the PQ KEM decapsulation key (private)
const PQ_KEM_DK_FILENAME: &str = "kem_dk.pq";

/// Filename for the PQ KEM encapsulation key (public)
const PQ_KEM_EK_FILENAME: &str = "kem_ek.pq";

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

    // ========== Iroh Key (Transport Layer) ==========

    /// Get the path to the iroh key file
    fn iroh_key_path(&self) -> PathBuf {
        self.path.join(IROH_KEY_FILENAME)
    }

    /// Load existing iroh key or generate a new one
    pub fn load_or_generate_iroh(&self) -> NodeResult<SecretKey> {
        let key_path = self.iroh_key_path();

        if key_path.exists() {
            self.load_iroh()
        } else {
            info!("No existing iroh identity found, generating new key");
            let key = SecretKey::generate(&mut rand::rng());
            self.save_iroh(&key)?;
            Ok(key)
        }
    }

    /// Load existing iroh key from disk
    pub fn load_iroh(&self) -> NodeResult<SecretKey> {
        let key_path = self.iroh_key_path();

        let bytes = std::fs::read(&key_path)
            .map_err(|e| NodeError::Keystore(format!("Failed to read iroh key file: {}", e)))?;

        if bytes.len() != 32 {
            return Err(NodeError::Keystore(format!(
                "Invalid iroh key file: expected 32 bytes, got {}",
                bytes.len()
            )));
        }

        let mut key_bytes = [0u8; 32];
        key_bytes.copy_from_slice(&bytes);

        let key = SecretKey::from_bytes(&key_bytes);
        debug!(
            identity = %key.public().fmt_short(),
            "Loaded iroh identity from keystore"
        );

        Ok(key)
    }

    /// Save iroh key to disk
    pub fn save_iroh(&self, key: &SecretKey) -> NodeResult<()> {
        std::fs::create_dir_all(&self.path)
            .map_err(|e| NodeError::Keystore(format!("Failed to create keystore dir: {}", e)))?;

        let key_path = self.iroh_key_path();
        std::fs::write(&key_path, key.to_bytes())
            .map_err(|e| NodeError::Keystore(format!("Failed to write iroh key file: {}", e)))?;

        Self::set_restrictive_permissions(&key_path)?;

        info!(
            identity = %key.public().fmt_short(),
            path = %key_path.display(),
            "Saved iroh identity to keystore"
        );

        Ok(())
    }

    // ========== PQ Identity (Dilithium3 / ML-DSA-65 Signatures) ==========

    /// Get the paths to the PQ identity files
    fn pq_signing_key_path(&self) -> PathBuf {
        self.path.join(PQ_SIGNING_KEY_FILENAME)
    }

    fn pq_verifying_key_path(&self) -> PathBuf {
        self.path.join(PQ_VERIFYING_KEY_FILENAME)
    }

    /// Load existing PQ identity or generate a new one
    pub fn load_or_generate_pq_identity(&self) -> NodeResult<PQIdentity> {
        let sk_path = self.pq_signing_key_path();
        let pk_path = self.pq_verifying_key_path();

        if sk_path.exists() && pk_path.exists() {
            self.load_pq_identity()
        } else {
            info!("No existing PQ identity found, generating new key pair");
            let identity = PQIdentity::generate();
            self.save_pq_identity(&identity)?;
            Ok(identity)
        }
    }

    /// Load existing PQ identity from disk
    pub fn load_pq_identity(&self) -> NodeResult<PQIdentity> {
        let sk_bytes = std::fs::read(self.pq_signing_key_path())
            .map_err(|e| NodeError::Keystore(format!("Failed to read PQ signing key file: {}", e)))?;

        let pk_bytes = std::fs::read(self.pq_verifying_key_path())
            .map_err(|e| NodeError::Keystore(format!("Failed to read PQ verifying key file: {}", e)))?;

        let identity = PQIdentity::from_keypair_bytes(&sk_bytes, &pk_bytes)
            .map_err(|e| NodeError::Keystore(format!("Invalid PQ identity files: {}", e)))?;

        debug!(
            pq_identity = %identity.verifying_key().short_id(),
            "Loaded PQ identity from keystore"
        );

        Ok(identity)
    }

    /// Save PQ identity to disk
    pub fn save_pq_identity(&self, identity: &PQIdentity) -> NodeResult<()> {
        std::fs::create_dir_all(&self.path)
            .map_err(|e| NodeError::Keystore(format!("Failed to create keystore dir: {}", e)))?;

        let (sk_bytes, pk_bytes) = identity.to_keypair_bytes();

        let sk_path = self.pq_signing_key_path();
        std::fs::write(&sk_path, sk_bytes.as_slice())
            .map_err(|e| NodeError::Keystore(format!("Failed to write PQ signing key file: {}", e)))?;
        Self::set_restrictive_permissions(&sk_path)?;

        let pk_path = self.pq_verifying_key_path();
        std::fs::write(&pk_path, &pk_bytes)
            .map_err(|e| NodeError::Keystore(format!("Failed to write PQ verifying key file: {}", e)))?;

        info!(
            pq_identity = %identity.verifying_key().short_id(),
            "Saved PQ identity to keystore"
        );

        Ok(())
    }

    // ========== PQ KEM Key Pair (Kyber768 / ML-KEM-768) ==========

    /// Get the paths to the PQ KEM key files
    fn pq_kem_dk_path(&self) -> PathBuf {
        self.path.join(PQ_KEM_DK_FILENAME)
    }

    fn pq_kem_ek_path(&self) -> PathBuf {
        self.path.join(PQ_KEM_EK_FILENAME)
    }

    /// Load existing PQ KEM key pair or generate a new one
    pub fn load_or_generate_pq_kem(&self) -> NodeResult<PQKemKeyPair> {
        let dk_path = self.pq_kem_dk_path();
        let ek_path = self.pq_kem_ek_path();

        if dk_path.exists() && ek_path.exists() {
            self.load_pq_kem()
        } else {
            info!("No existing PQ KEM key found, generating new key pair");
            let keypair = PQKemKeyPair::generate();
            self.save_pq_kem(&keypair)?;
            Ok(keypair)
        }
    }

    /// Load existing PQ KEM key pair from disk
    pub fn load_pq_kem(&self) -> NodeResult<PQKemKeyPair> {
        let dk_bytes = std::fs::read(self.pq_kem_dk_path())
            .map_err(|e| NodeError::Keystore(format!("Failed to read PQ KEM decapsulation key file: {}", e)))?;

        let ek_bytes = std::fs::read(self.pq_kem_ek_path())
            .map_err(|e| NodeError::Keystore(format!("Failed to read PQ KEM encapsulation key file: {}", e)))?;

        let keypair = PQKemKeyPair::from_keypair_bytes(&dk_bytes, &ek_bytes)
            .map_err(|e| NodeError::Keystore(format!("Invalid PQ KEM key files: {}", e)))?;

        debug!(
            pq_kem = %keypair.encapsulation_key().short_id(),
            "Loaded PQ KEM key pair from keystore"
        );

        Ok(keypair)
    }

    /// Save PQ KEM key pair to disk
    pub fn save_pq_kem(&self, keypair: &PQKemKeyPair) -> NodeResult<()> {
        std::fs::create_dir_all(&self.path)
            .map_err(|e| NodeError::Keystore(format!("Failed to create keystore dir: {}", e)))?;

        let (dk_bytes, ek_bytes) = keypair.to_keypair_bytes();

        let dk_path = self.pq_kem_dk_path();
        std::fs::write(&dk_path, dk_bytes.as_slice())
            .map_err(|e| NodeError::Keystore(format!("Failed to write PQ KEM decapsulation key file: {}", e)))?;
        Self::set_restrictive_permissions(&dk_path)?;

        let ek_path = self.pq_kem_ek_path();
        std::fs::write(&ek_path, &ek_bytes)
            .map_err(|e| NodeError::Keystore(format!("Failed to write PQ KEM encapsulation key file: {}", e)))?;

        info!(
            pq_kem = %keypair.encapsulation_key().short_id(),
            "Saved PQ KEM key pair to keystore"
        );

        Ok(())
    }

    // ========== Legacy Methods (backward compatibility) ==========

    /// Load existing key or generate a new one (legacy - use load_or_generate_iroh)
    pub fn load_or_generate(&self) -> NodeResult<SecretKey> {
        self.load_or_generate_iroh()
    }

    /// Load an existing key from disk (legacy - use load_iroh)
    pub fn load(&self) -> NodeResult<SecretKey> {
        self.load_iroh()
    }

    /// Save a key to disk (legacy - use save_iroh)
    pub fn save(&self, key: &SecretKey) -> NodeResult<()> {
        self.save_iroh(key)
    }

    // ========== Utility Methods ==========

    /// Check if an iroh key file exists
    pub fn exists(&self) -> bool {
        self.iroh_key_path().exists()
    }

    /// Check if PQ identity files exist
    pub fn pq_identity_exists(&self) -> bool {
        self.pq_signing_key_path().exists() && self.pq_verifying_key_path().exists()
    }

    /// Check if PQ KEM key files exist
    pub fn pq_kem_exists(&self) -> bool {
        self.pq_kem_dk_path().exists() && self.pq_kem_ek_path().exists()
    }

    /// Delete all key files (use with caution!)
    pub fn delete(&self) -> NodeResult<()> {
        self.delete_file(&self.iroh_key_path())?;
        self.delete_file(&self.pq_signing_key_path())?;
        self.delete_file(&self.pq_verifying_key_path())?;
        self.delete_file(&self.pq_kem_dk_path())?;
        self.delete_file(&self.pq_kem_ek_path())?;
        Ok(())
    }

    /// Delete a single key file
    fn delete_file(&self, path: &PathBuf) -> NodeResult<()> {
        if path.exists() {
            std::fs::remove_file(path)
                .map_err(|e| NodeError::Keystore(format!("Failed to delete key file: {}", e)))?;
        }
        Ok(())
    }

    /// Set restrictive permissions on a key file (Unix only)
    fn set_restrictive_permissions(path: &PathBuf) -> NodeResult<()> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            std::fs::set_permissions(path, perms)
                .map_err(|e| NodeError::Keystore(format!("Failed to set key permissions: {}", e)))?;
        }
        let _ = path; // Silence unused warning on non-Unix
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_iroh_generate_and_load() {
        let temp_dir = TempDir::new().unwrap();
        let keystore = Keystore::new(temp_dir.path());

        // Should not exist initially
        assert!(!keystore.exists());

        // Generate new key
        let key1 = keystore.load_or_generate_iroh().unwrap();
        assert!(keystore.exists());

        // Load same key
        let key2 = keystore.load_or_generate_iroh().unwrap();
        assert_eq!(key1.public(), key2.public());

        // Explicit load should also work
        let key3 = keystore.load_iroh().unwrap();
        assert_eq!(key1.public(), key3.public());
    }

    #[test]
    fn test_iroh_save_and_load() {
        let temp_dir = TempDir::new().unwrap();
        let keystore = Keystore::new(temp_dir.path());

        let original_key = SecretKey::generate(&mut rand::rng());
        keystore.save_iroh(&original_key).unwrap();

        let loaded_key = keystore.load_iroh().unwrap();
        assert_eq!(original_key.public(), loaded_key.public());
    }

    #[test]
    fn test_pq_identity_generate_and_load() {
        let temp_dir = TempDir::new().unwrap();
        let keystore = Keystore::new(temp_dir.path());

        // Should not exist initially
        assert!(!keystore.pq_identity_exists());

        // Generate new identity
        let identity1 = keystore.load_or_generate_pq_identity().unwrap();
        assert!(keystore.pq_identity_exists());

        // Load same identity
        let identity2 = keystore.load_or_generate_pq_identity().unwrap();
        assert_eq!(
            identity1.verifying_key_bytes(),
            identity2.verifying_key_bytes()
        );

        // Signatures should be verifiable
        let message = b"Test message";
        let signature = identity1.sign(message);
        assert!(identity2.verify(message, &signature));
    }

    #[test]
    fn test_pq_kem_generate_and_load() {
        let temp_dir = TempDir::new().unwrap();
        let keystore = Keystore::new(temp_dir.path());

        // Should not exist initially
        assert!(!keystore.pq_kem_exists());

        // Generate new key pair
        let keypair1 = keystore.load_or_generate_pq_kem().unwrap();
        assert!(keystore.pq_kem_exists());

        // Load same key pair
        let keypair2 = keystore.load_or_generate_pq_kem().unwrap();
        assert_eq!(
            keypair1.encapsulation_key_bytes(),
            keypair2.encapsulation_key_bytes()
        );

        // Decapsulation should work
        let (ciphertext, secret1) = keypair1.encapsulation_key().encapsulate();
        let secret2 = keypair2.decapsulate(&ciphertext).unwrap();
        assert_eq!(secret1, secret2);
    }

    #[test]
    fn test_delete_all_keys() {
        let temp_dir = TempDir::new().unwrap();
        let keystore = Keystore::new(temp_dir.path());

        // Generate all key types
        keystore.load_or_generate_iroh().unwrap();
        keystore.load_or_generate_pq_identity().unwrap();
        keystore.load_or_generate_pq_kem().unwrap();

        assert!(keystore.exists());
        assert!(keystore.pq_identity_exists());
        assert!(keystore.pq_kem_exists());

        // Delete all
        keystore.delete().unwrap();

        assert!(!keystore.exists());
        assert!(!keystore.pq_identity_exists());
        assert!(!keystore.pq_kem_exists());
    }

    #[test]
    fn test_invalid_iroh_key_file() {
        let temp_dir = TempDir::new().unwrap();
        let keystore = Keystore::new(temp_dir.path());

        // Write invalid data
        std::fs::create_dir_all(temp_dir.path()).unwrap();
        std::fs::write(temp_dir.path().join(IROH_KEY_FILENAME), b"too short").unwrap();

        let result = keystore.load_iroh();
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_pq_identity_file() {
        let temp_dir = TempDir::new().unwrap();
        let keystore = Keystore::new(temp_dir.path());

        // Write invalid data
        std::fs::create_dir_all(temp_dir.path()).unwrap();
        std::fs::write(temp_dir.path().join(PQ_SIGNING_KEY_FILENAME), b"too short").unwrap();
        std::fs::write(temp_dir.path().join(PQ_VERIFYING_KEY_FILENAME), b"too short").unwrap();

        let result = keystore.load_pq_identity();
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_pq_kem_file() {
        let temp_dir = TempDir::new().unwrap();
        let keystore = Keystore::new(temp_dir.path());

        // Write invalid data
        std::fs::create_dir_all(temp_dir.path()).unwrap();
        std::fs::write(temp_dir.path().join(PQ_KEM_DK_FILENAME), b"too short").unwrap();
        std::fs::write(temp_dir.path().join(PQ_KEM_EK_FILENAME), b"too short").unwrap();

        let result = keystore.load_pq_kem();
        assert!(result.is_err());
    }

    // Legacy compatibility tests
    #[test]
    fn test_legacy_methods() {
        let temp_dir = TempDir::new().unwrap();
        let keystore = Keystore::new(temp_dir.path());

        let original_key = SecretKey::generate(&mut rand::rng());
        keystore.save(&original_key).unwrap(); // Legacy save

        let loaded_key = keystore.load().unwrap(); // Legacy load
        assert_eq!(original_key.public(), loaded_key.public());

        let loaded_key2 = keystore.load_or_generate().unwrap(); // Legacy load_or_generate
        assert_eq!(original_key.public(), loaded_key2.public());
    }
}
