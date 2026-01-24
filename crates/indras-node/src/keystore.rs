//! Keystore for identity persistence
//!
//! Handles loading and saving the node's keys to disk,
//! ensuring identity persists across node restarts.
//!
//! Stores:
//! - Iroh Ed25519 key (for transport layer)
//! - PQ identity (ML-DSA-65 for signatures)
//! - PQ KEM key pair (ML-KEM-768 for key encapsulation)
//!
//! ## Encryption
//!
//! Keys can be optionally encrypted at rest using passphrase-based encryption:
//! - Key derivation: Argon2id with secure parameters
//! - Encryption: ChaCha20-Poly1305 authenticated encryption

use std::path::{Path, PathBuf};

use argon2::{Argon2, Params, Version};
use chacha20poly1305::{
    ChaCha20Poly1305, Nonce,
    aead::{Aead, KeyInit},
};
use iroh::SecretKey;
use rand::RngCore;
use tracing::{debug, info, warn};

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

/// Filename suffix for encrypted key files
#[allow(dead_code)] // Reserved for future encrypted keystore feature
const ENCRYPTED_SUFFIX: &str = ".enc";

/// Filename for the encryption salt
#[allow(dead_code)] // Reserved for future encrypted keystore feature
const SALT_FILENAME: &str = "keystore.salt";

/// Nonce size for ChaCha20-Poly1305 (12 bytes)
#[allow(dead_code)] // Reserved for future encrypted keystore feature
const NONCE_SIZE: usize = 12;

/// Key size for encryption (32 bytes)
#[allow(dead_code)] // Reserved for future encrypted keystore feature
const ENCRYPTION_KEY_SIZE: usize = 32;

/// Keystore for managing node identity persistence
///
/// The keystore saves and loads the node's secret key from disk,
/// allowing the node to maintain the same identity across restarts.
pub struct Keystore {
    /// Path to the keystore directory
    pub(crate) path: PathBuf,
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
    pub(crate) fn iroh_key_path(&self) -> PathBuf {
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
    pub(crate) fn pq_signing_key_path(&self) -> PathBuf {
        self.path.join(PQ_SIGNING_KEY_FILENAME)
    }

    pub(crate) fn pq_verifying_key_path(&self) -> PathBuf {
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
        let sk_bytes = std::fs::read(self.pq_signing_key_path()).map_err(|e| {
            NodeError::Keystore(format!("Failed to read PQ signing key file: {}", e))
        })?;

        let pk_bytes = std::fs::read(self.pq_verifying_key_path()).map_err(|e| {
            NodeError::Keystore(format!("Failed to read PQ verifying key file: {}", e))
        })?;

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
        std::fs::write(&sk_path, sk_bytes.as_slice()).map_err(|e| {
            NodeError::Keystore(format!("Failed to write PQ signing key file: {}", e))
        })?;
        Self::set_restrictive_permissions(&sk_path)?;

        let pk_path = self.pq_verifying_key_path();
        std::fs::write(&pk_path, &pk_bytes).map_err(|e| {
            NodeError::Keystore(format!("Failed to write PQ verifying key file: {}", e))
        })?;

        info!(
            pq_identity = %identity.verifying_key().short_id(),
            "Saved PQ identity to keystore"
        );

        Ok(())
    }

    // ========== PQ KEM Key Pair (Kyber768 / ML-KEM-768) ==========

    /// Get the paths to the PQ KEM key files
    pub(crate) fn pq_kem_dk_path(&self) -> PathBuf {
        self.path.join(PQ_KEM_DK_FILENAME)
    }

    pub(crate) fn pq_kem_ek_path(&self) -> PathBuf {
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
        let dk_bytes = std::fs::read(self.pq_kem_dk_path()).map_err(|e| {
            NodeError::Keystore(format!(
                "Failed to read PQ KEM decapsulation key file: {}",
                e
            ))
        })?;

        let ek_bytes = std::fs::read(self.pq_kem_ek_path()).map_err(|e| {
            NodeError::Keystore(format!(
                "Failed to read PQ KEM encapsulation key file: {}",
                e
            ))
        })?;

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
        std::fs::write(&dk_path, dk_bytes.as_slice()).map_err(|e| {
            NodeError::Keystore(format!(
                "Failed to write PQ KEM decapsulation key file: {}",
                e
            ))
        })?;
        Self::set_restrictive_permissions(&dk_path)?;

        let ek_path = self.pq_kem_ek_path();
        std::fs::write(&ek_path, &ek_bytes).map_err(|e| {
            NodeError::Keystore(format!(
                "Failed to write PQ KEM encapsulation key file: {}",
                e
            ))
        })?;

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
    pub(crate) fn set_restrictive_permissions(path: &PathBuf) -> NodeResult<()> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            std::fs::set_permissions(path, perms).map_err(|e| {
                NodeError::Keystore(format!("Failed to set key permissions: {}", e))
            })?;
        }
        let _ = path; // Silence unused warning on non-Unix
        Ok(())
    }
}

/// Encrypted keystore using passphrase-based encryption
///
/// Wraps a [`Keystore`] and provides transparent encryption/decryption
/// of keys using Argon2id key derivation and ChaCha20-Poly1305.
///
/// ## Usage
///
/// ```ignore
/// let encrypted = EncryptedKeystore::new(data_dir);
///
/// // First time setup - this will prompt for passphrase
/// encrypted.unlock("my-secure-passphrase")?;
///
/// // Now use like regular keystore
/// let iroh_key = encrypted.load_or_generate_iroh()?;
/// ```
#[allow(dead_code)] // Reserved for future encrypted keystore feature
pub struct EncryptedKeystore {
    /// The underlying unencrypted keystore
    inner: Keystore,
    /// Derived encryption key (32 bytes, from passphrase)
    encryption_key: Option<[u8; ENCRYPTION_KEY_SIZE]>,
    /// Whether encryption is enabled
    encrypted: bool,
}

#[allow(dead_code)] // Reserved for future encrypted keystore feature
impl EncryptedKeystore {
    /// Create a new encrypted keystore
    ///
    /// The keystore starts locked. Call [`unlock`] with a passphrase before use.
    pub fn new(data_dir: &Path) -> Self {
        Self {
            inner: Keystore::new(data_dir),
            encryption_key: None,
            encrypted: true,
        }
    }

    /// Create an unencrypted keystore (for backward compatibility)
    ///
    /// Keys will be stored in plaintext (protected only by file permissions).
    pub fn new_unencrypted(data_dir: &Path) -> Self {
        Self {
            inner: Keystore::new(data_dir),
            encryption_key: None,
            encrypted: false,
        }
    }

    /// Check if the keystore is locked
    pub fn is_locked(&self) -> bool {
        self.encrypted && self.encryption_key.is_none()
    }

    /// Check if encryption is enabled
    pub fn is_encrypted(&self) -> bool {
        self.encrypted
    }

    /// Unlock the keystore with a passphrase
    ///
    /// Derives an encryption key using Argon2id and stores it for subsequent operations.
    /// If a salt file doesn't exist, one will be created.
    pub fn unlock(&mut self, passphrase: &str) -> NodeResult<()> {
        if !self.encrypted {
            // Unencrypted mode - nothing to do
            return Ok(());
        }

        if passphrase.is_empty() {
            return Err(NodeError::Keystore(
                "Passphrase cannot be empty".to_string(),
            ));
        }

        let salt = self.load_or_create_salt()?;
        let key = Self::derive_key(passphrase, &salt)?;
        self.encryption_key = Some(key);

        info!("Keystore unlocked successfully");
        Ok(())
    }

    /// Lock the keystore (clear the encryption key from memory)
    pub fn lock(&mut self) {
        if let Some(ref mut key) = self.encryption_key {
            // Zeroize the key
            key.fill(0);
        }
        self.encryption_key = None;
        debug!("Keystore locked");
    }

    /// Change the passphrase for the keystore
    ///
    /// This re-encrypts all existing keys with the new passphrase.
    /// The keystore must be unlocked first.
    pub fn change_passphrase(&mut self, new_passphrase: &str) -> NodeResult<()> {
        if self.is_locked() {
            return Err(NodeError::Keystore("Keystore is locked".to_string()));
        }

        if new_passphrase.is_empty() {
            return Err(NodeError::Keystore(
                "New passphrase cannot be empty".to_string(),
            ));
        }

        // Load all existing keys (check both encrypted and unencrypted paths)
        let iroh_key = if self.encrypted_iroh_key_exists() || self.inner.exists() {
            Some(self.load_iroh()?)
        } else {
            None
        };

        let pq_identity = if self.encrypted_pq_identity_exists() || self.inner.pq_identity_exists()
        {
            Some(self.load_pq_identity()?)
        } else {
            None
        };

        let pq_kem = if self.encrypted_pq_kem_exists() || self.inner.pq_kem_exists() {
            Some(self.load_pq_kem()?)
        } else {
            None
        };

        // Generate new salt and derive new key
        let salt = self.create_new_salt()?;
        let new_key = Self::derive_key(new_passphrase, &salt)?;
        self.encryption_key = Some(new_key);

        // Re-encrypt and save all keys
        if let Some(key) = iroh_key {
            self.save_iroh(&key)?;
        }

        if let Some(identity) = pq_identity {
            self.save_pq_identity(&identity)?;
        }

        if let Some(kem) = pq_kem {
            self.save_pq_kem(&kem)?;
        }

        info!("Passphrase changed successfully");
        Ok(())
    }

    /// Migrate from unencrypted to encrypted storage
    ///
    /// Reads plaintext keys and re-saves them encrypted.
    pub fn migrate_to_encrypted(&mut self, passphrase: &str) -> NodeResult<()> {
        if self.encrypted && self.encryption_key.is_some() {
            return Err(NodeError::Keystore("Already encrypted".to_string()));
        }

        // Temporarily disable encryption to read plaintext keys
        self.encrypted = false;

        let iroh_key = if self.inner.exists() {
            Some(self.load_iroh()?)
        } else {
            None
        };

        let pq_identity = if self.inner.pq_identity_exists() {
            Some(self.load_pq_identity()?)
        } else {
            None
        };

        let pq_kem = if self.inner.pq_kem_exists() {
            Some(self.load_pq_kem()?)
        } else {
            None
        };

        // Enable encryption and unlock
        self.encrypted = true;
        self.unlock(passphrase)?;

        // Re-save all keys encrypted
        if let Some(key) = iroh_key {
            self.save_iroh(&key)?;
            // Remove old plaintext file
            let _ = std::fs::remove_file(self.inner.iroh_key_path());
        }

        if let Some(identity) = pq_identity {
            self.save_pq_identity(&identity)?;
            // Remove old plaintext files
            let _ = std::fs::remove_file(self.inner.pq_signing_key_path());
            let _ = std::fs::remove_file(self.inner.pq_verifying_key_path());
        }

        if let Some(kem) = pq_kem {
            self.save_pq_kem(&kem)?;
            // Remove old plaintext files
            let _ = std::fs::remove_file(self.inner.pq_kem_dk_path());
            let _ = std::fs::remove_file(self.inner.pq_kem_ek_path());
        }

        info!("Migrated keystore to encrypted storage");
        Ok(())
    }

    // ========== Iroh Key Operations ==========

    /// Load existing iroh key or generate a new one
    pub fn load_or_generate_iroh(&self) -> NodeResult<SecretKey> {
        self.ensure_unlocked()?;

        if self.encrypted_iroh_key_exists() || self.inner.exists() {
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
        self.ensure_unlocked()?;

        if self.encrypted {
            self.load_iroh_encrypted()
        } else {
            self.inner.load_iroh()
        }
    }

    /// Save iroh key to disk
    pub fn save_iroh(&self, key: &SecretKey) -> NodeResult<()> {
        self.ensure_unlocked()?;

        if self.encrypted {
            self.save_iroh_encrypted(key)
        } else {
            self.inner.save_iroh(key)
        }
    }

    // ========== PQ Identity Operations ==========

    /// Load existing PQ identity or generate a new one
    pub fn load_or_generate_pq_identity(&self) -> NodeResult<PQIdentity> {
        self.ensure_unlocked()?;

        if self.encrypted_pq_identity_exists() || self.inner.pq_identity_exists() {
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
        self.ensure_unlocked()?;

        if self.encrypted {
            self.load_pq_identity_encrypted()
        } else {
            self.inner.load_pq_identity()
        }
    }

    /// Save PQ identity to disk
    pub fn save_pq_identity(&self, identity: &PQIdentity) -> NodeResult<()> {
        self.ensure_unlocked()?;

        if self.encrypted {
            self.save_pq_identity_encrypted(identity)
        } else {
            self.inner.save_pq_identity(identity)
        }
    }

    // ========== PQ KEM Operations ==========

    /// Load existing PQ KEM key pair or generate a new one
    pub fn load_or_generate_pq_kem(&self) -> NodeResult<PQKemKeyPair> {
        self.ensure_unlocked()?;

        if self.encrypted_pq_kem_exists() || self.inner.pq_kem_exists() {
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
        self.ensure_unlocked()?;

        if self.encrypted {
            self.load_pq_kem_encrypted()
        } else {
            self.inner.load_pq_kem()
        }
    }

    /// Save PQ KEM key pair to disk
    pub fn save_pq_kem(&self, keypair: &PQKemKeyPair) -> NodeResult<()> {
        self.ensure_unlocked()?;

        if self.encrypted {
            self.save_pq_kem_encrypted(keypair)
        } else {
            self.inner.save_pq_kem(keypair)
        }
    }

    // ========== Existence Checks ==========

    /// Check if encrypted iroh key file exists
    pub fn encrypted_iroh_key_exists(&self) -> bool {
        self.encrypted_path(IROH_KEY_FILENAME).exists()
    }

    /// Check if encrypted PQ identity files exist
    pub fn encrypted_pq_identity_exists(&self) -> bool {
        self.encrypted_path(PQ_SIGNING_KEY_FILENAME).exists()
            && self.encrypted_path(PQ_VERIFYING_KEY_FILENAME).exists()
    }

    /// Check if encrypted PQ KEM key files exist
    pub fn encrypted_pq_kem_exists(&self) -> bool {
        self.encrypted_path(PQ_KEM_DK_FILENAME).exists()
            && self.encrypted_path(PQ_KEM_EK_FILENAME).exists()
    }

    /// Check if any keys exist (encrypted or plaintext)
    pub fn exists(&self) -> bool {
        self.encrypted_iroh_key_exists() || self.inner.exists()
    }

    // ========== Private Helpers ==========

    /// Get the path for an encrypted key file
    fn encrypted_path(&self, filename: &str) -> PathBuf {
        self.inner
            .path
            .join(format!("{}{}", filename, ENCRYPTED_SUFFIX))
    }

    /// Get the salt file path
    fn salt_path(&self) -> PathBuf {
        self.inner.path.join(SALT_FILENAME)
    }

    /// Load existing salt or create a new one
    fn load_or_create_salt(&self) -> NodeResult<Vec<u8>> {
        let salt_path = self.salt_path();

        if salt_path.exists() {
            std::fs::read(&salt_path)
                .map_err(|e| NodeError::Keystore(format!("Failed to read salt file: {}", e)))
        } else {
            self.create_new_salt()
        }
    }

    /// Create a new random salt and save it
    fn create_new_salt(&self) -> NodeResult<Vec<u8>> {
        std::fs::create_dir_all(&self.inner.path)
            .map_err(|e| NodeError::Keystore(format!("Failed to create keystore dir: {}", e)))?;

        // Generate 16 bytes of random salt (recommended minimum for Argon2)
        let mut salt_bytes = vec![0u8; 16];
        rand::rng().fill_bytes(&mut salt_bytes);

        let salt_path = self.salt_path();
        std::fs::write(&salt_path, &salt_bytes)
            .map_err(|e| NodeError::Keystore(format!("Failed to write salt file: {}", e)))?;

        Keystore::set_restrictive_permissions(&salt_path)?;

        debug!("Created new keystore salt");
        Ok(salt_bytes)
    }

    /// Derive encryption key from passphrase using Argon2id
    fn derive_key(passphrase: &str, salt: &[u8]) -> NodeResult<[u8; ENCRYPTION_KEY_SIZE]> {
        // Use secure Argon2id parameters (OWASP recommendations)
        // - m_cost: 19456 KiB (19 MiB)
        // - t_cost: 2 iterations
        // - p_cost: 1 thread (can be increased for parallelism)
        let params = Params::new(19456, 2, 1, Some(ENCRYPTION_KEY_SIZE))
            .map_err(|e| NodeError::Keystore(format!("Invalid Argon2 params: {}", e)))?;

        let argon2 = Argon2::new(argon2::Algorithm::Argon2id, Version::V0x13, params);

        let mut key = [0u8; ENCRYPTION_KEY_SIZE];
        argon2
            .hash_password_into(passphrase.as_bytes(), salt, &mut key)
            .map_err(|e| NodeError::Keystore(format!("Failed to derive key: {}", e)))?;

        Ok(key)
    }

    /// Ensure the keystore is unlocked
    fn ensure_unlocked(&self) -> NodeResult<()> {
        if self.is_locked() {
            Err(NodeError::Keystore(
                "Keystore is locked. Call unlock() with passphrase first.".to_string(),
            ))
        } else {
            Ok(())
        }
    }

    /// Encrypt data with the derived key
    fn encrypt(&self, plaintext: &[u8]) -> NodeResult<Vec<u8>> {
        let key = self
            .encryption_key
            .as_ref()
            .ok_or_else(|| NodeError::Keystore("No encryption key".to_string()))?;

        let cipher = ChaCha20Poly1305::new_from_slice(key)
            .map_err(|e| NodeError::Keystore(format!("Failed to create cipher: {}", e)))?;

        // Generate random nonce
        let mut nonce_bytes = [0u8; NONCE_SIZE];
        rand::rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = cipher
            .encrypt(nonce, plaintext)
            .map_err(|e| NodeError::Keystore(format!("Encryption failed: {}", e)))?;

        // Prepend nonce to ciphertext
        let mut result = Vec::with_capacity(NONCE_SIZE + ciphertext.len());
        result.extend_from_slice(&nonce_bytes);
        result.extend_from_slice(&ciphertext);
        Ok(result)
    }

    /// Decrypt data with the derived key
    fn decrypt(&self, data: &[u8]) -> NodeResult<Vec<u8>> {
        if data.len() < NONCE_SIZE {
            return Err(NodeError::Keystore("Data too short".to_string()));
        }

        let key = self
            .encryption_key
            .as_ref()
            .ok_or_else(|| NodeError::Keystore("No encryption key".to_string()))?;

        let cipher = ChaCha20Poly1305::new_from_slice(key)
            .map_err(|e| NodeError::Keystore(format!("Failed to create cipher: {}", e)))?;

        let nonce = Nonce::from_slice(&data[..NONCE_SIZE]);
        let ciphertext = &data[NONCE_SIZE..];

        cipher.decrypt(nonce, ciphertext).map_err(|e| {
            NodeError::Keystore(format!("Decryption failed (wrong passphrase?): {}", e))
        })
    }

    /// Save encrypted data to a file
    fn save_encrypted_file(&self, path: &PathBuf, plaintext: &[u8]) -> NodeResult<()> {
        std::fs::create_dir_all(&self.inner.path)
            .map_err(|e| NodeError::Keystore(format!("Failed to create keystore dir: {}", e)))?;

        let encrypted = self.encrypt(plaintext)?;
        std::fs::write(path, encrypted)
            .map_err(|e| NodeError::Keystore(format!("Failed to write file: {}", e)))?;

        Keystore::set_restrictive_permissions(path)?;
        Ok(())
    }

    /// Load and decrypt data from a file
    fn load_encrypted_file(&self, path: &PathBuf) -> NodeResult<Vec<u8>> {
        let data = std::fs::read(path)
            .map_err(|e| NodeError::Keystore(format!("Failed to read file: {}", e)))?;

        self.decrypt(&data)
    }

    // ========== Encrypted Iroh Key Operations ==========

    fn save_iroh_encrypted(&self, key: &SecretKey) -> NodeResult<()> {
        let path = self.encrypted_path(IROH_KEY_FILENAME);
        self.save_encrypted_file(&path, &key.to_bytes())?;

        info!(
            identity = %key.public().fmt_short(),
            path = %path.display(),
            "Saved encrypted iroh identity"
        );
        Ok(())
    }

    fn load_iroh_encrypted(&self) -> NodeResult<SecretKey> {
        let path = self.encrypted_path(IROH_KEY_FILENAME);

        // Try encrypted file first, fall back to plaintext for migration
        let bytes = if path.exists() {
            self.load_encrypted_file(&path)?
        } else if self.inner.iroh_key_path().exists() {
            warn!("Found plaintext key file, consider running migration");
            std::fs::read(self.inner.iroh_key_path())
                .map_err(|e| NodeError::Keystore(format!("Failed to read key file: {}", e)))?
        } else {
            return Err(NodeError::Keystore("No iroh key file found".to_string()));
        };

        if bytes.len() != 32 {
            return Err(NodeError::Keystore(format!(
                "Invalid iroh key: expected 32 bytes, got {}",
                bytes.len()
            )));
        }

        let mut key_bytes = [0u8; 32];
        key_bytes.copy_from_slice(&bytes);

        let key = SecretKey::from_bytes(&key_bytes);
        debug!(
            identity = %key.public().fmt_short(),
            "Loaded encrypted iroh identity"
        );

        Ok(key)
    }

    // ========== Encrypted PQ Identity Operations ==========

    fn save_pq_identity_encrypted(&self, identity: &PQIdentity) -> NodeResult<()> {
        let (sk_bytes, pk_bytes) = identity.to_keypair_bytes();

        // Encrypt private key
        let sk_path = self.encrypted_path(PQ_SIGNING_KEY_FILENAME);
        self.save_encrypted_file(&sk_path, sk_bytes.as_slice())?;

        // Public key doesn't need encryption, but we encrypt for consistency
        let pk_path = self.encrypted_path(PQ_VERIFYING_KEY_FILENAME);
        self.save_encrypted_file(&pk_path, &pk_bytes)?;

        info!(
            pq_identity = %identity.verifying_key().short_id(),
            "Saved encrypted PQ identity"
        );
        Ok(())
    }

    fn load_pq_identity_encrypted(&self) -> NodeResult<PQIdentity> {
        let sk_path = self.encrypted_path(PQ_SIGNING_KEY_FILENAME);
        let pk_path = self.encrypted_path(PQ_VERIFYING_KEY_FILENAME);

        // Try encrypted files first, fall back to plaintext
        let sk_bytes = if sk_path.exists() {
            self.load_encrypted_file(&sk_path)?
        } else if self.inner.pq_signing_key_path().exists() {
            warn!("Found plaintext PQ key file, consider running migration");
            std::fs::read(self.inner.pq_signing_key_path())
                .map_err(|e| NodeError::Keystore(format!("Failed to read file: {}", e)))?
        } else {
            return Err(NodeError::Keystore(
                "No PQ signing key file found".to_string(),
            ));
        };

        let pk_bytes = if pk_path.exists() {
            self.load_encrypted_file(&pk_path)?
        } else if self.inner.pq_verifying_key_path().exists() {
            std::fs::read(self.inner.pq_verifying_key_path())
                .map_err(|e| NodeError::Keystore(format!("Failed to read file: {}", e)))?
        } else {
            return Err(NodeError::Keystore(
                "No PQ verifying key file found".to_string(),
            ));
        };

        let identity = PQIdentity::from_keypair_bytes(&sk_bytes, &pk_bytes)
            .map_err(|e| NodeError::Keystore(format!("Invalid PQ identity: {}", e)))?;

        debug!(
            pq_identity = %identity.verifying_key().short_id(),
            "Loaded encrypted PQ identity"
        );

        Ok(identity)
    }

    // ========== Encrypted PQ KEM Operations ==========

    fn save_pq_kem_encrypted(&self, keypair: &PQKemKeyPair) -> NodeResult<()> {
        let (dk_bytes, ek_bytes) = keypair.to_keypair_bytes();

        // Encrypt private key
        let dk_path = self.encrypted_path(PQ_KEM_DK_FILENAME);
        self.save_encrypted_file(&dk_path, dk_bytes.as_slice())?;

        // Public key doesn't need encryption, but we encrypt for consistency
        let ek_path = self.encrypted_path(PQ_KEM_EK_FILENAME);
        self.save_encrypted_file(&ek_path, &ek_bytes)?;

        info!(
            pq_kem = %keypair.encapsulation_key().short_id(),
            "Saved encrypted PQ KEM key pair"
        );
        Ok(())
    }

    fn load_pq_kem_encrypted(&self) -> NodeResult<PQKemKeyPair> {
        let dk_path = self.encrypted_path(PQ_KEM_DK_FILENAME);
        let ek_path = self.encrypted_path(PQ_KEM_EK_FILENAME);

        // Try encrypted files first, fall back to plaintext
        let dk_bytes = if dk_path.exists() {
            self.load_encrypted_file(&dk_path)?
        } else if self.inner.pq_kem_dk_path().exists() {
            warn!("Found plaintext PQ KEM key file, consider running migration");
            std::fs::read(self.inner.pq_kem_dk_path())
                .map_err(|e| NodeError::Keystore(format!("Failed to read file: {}", e)))?
        } else {
            return Err(NodeError::Keystore(
                "No PQ KEM decapsulation key file found".to_string(),
            ));
        };

        let ek_bytes = if ek_path.exists() {
            self.load_encrypted_file(&ek_path)?
        } else if self.inner.pq_kem_ek_path().exists() {
            std::fs::read(self.inner.pq_kem_ek_path())
                .map_err(|e| NodeError::Keystore(format!("Failed to read file: {}", e)))?
        } else {
            return Err(NodeError::Keystore(
                "No PQ KEM encapsulation key file found".to_string(),
            ));
        };

        let keypair = PQKemKeyPair::from_keypair_bytes(&dk_bytes, &ek_bytes)
            .map_err(|e| NodeError::Keystore(format!("Invalid PQ KEM key pair: {}", e)))?;

        debug!(
            pq_kem = %keypair.encapsulation_key().short_id(),
            "Loaded encrypted PQ KEM key pair"
        );

        Ok(keypair)
    }
}

impl Drop for EncryptedKeystore {
    fn drop(&mut self) {
        // Zeroize the encryption key on drop
        self.lock();
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
        std::fs::write(
            temp_dir.path().join(PQ_VERIFYING_KEY_FILENAME),
            b"too short",
        )
        .unwrap();

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

    // ========== Encrypted Keystore Tests ==========

    #[test]
    fn test_encrypted_keystore_locked_by_default() {
        let temp_dir = TempDir::new().unwrap();
        let keystore = EncryptedKeystore::new(temp_dir.path());

        assert!(keystore.is_locked());
        assert!(keystore.is_encrypted());

        // Operations should fail when locked
        let result = keystore.load_or_generate_iroh();
        assert!(result.is_err());
    }

    #[test]
    fn test_encrypted_keystore_unlock() {
        let temp_dir = TempDir::new().unwrap();
        let mut keystore = EncryptedKeystore::new(temp_dir.path());

        keystore.unlock("test-passphrase").unwrap();
        assert!(!keystore.is_locked());

        // Now operations should work
        let key = keystore.load_or_generate_iroh().unwrap();
        assert!(keystore.encrypted_iroh_key_exists());

        // Load should return same key
        let key2 = keystore.load_iroh().unwrap();
        assert_eq!(key.public(), key2.public());
    }

    #[test]
    fn test_encrypted_keystore_wrong_passphrase() {
        let temp_dir = TempDir::new().unwrap();

        // Create and save a key with one passphrase
        {
            let mut keystore = EncryptedKeystore::new(temp_dir.path());
            keystore.unlock("correct-passphrase").unwrap();
            keystore.load_or_generate_iroh().unwrap();
        }

        // Try to load with wrong passphrase
        {
            let mut keystore = EncryptedKeystore::new(temp_dir.path());
            keystore.unlock("wrong-passphrase").unwrap();

            let result = keystore.load_iroh();
            assert!(result.is_err());
        }
    }

    #[test]
    fn test_encrypted_keystore_pq_identity() {
        let temp_dir = TempDir::new().unwrap();
        let mut keystore = EncryptedKeystore::new(temp_dir.path());
        keystore.unlock("test-passphrase").unwrap();

        // Generate and save PQ identity
        let identity = keystore.load_or_generate_pq_identity().unwrap();
        assert!(keystore.encrypted_pq_identity_exists());

        // Load and verify
        let identity2 = keystore.load_pq_identity().unwrap();
        assert_eq!(
            identity.verifying_key_bytes(),
            identity2.verifying_key_bytes()
        );

        // Test signing
        let message = b"Test message";
        let signature = identity.sign(message);
        assert!(identity2.verify(message, &signature));
    }

    #[test]
    fn test_encrypted_keystore_pq_kem() {
        let temp_dir = TempDir::new().unwrap();
        let mut keystore = EncryptedKeystore::new(temp_dir.path());
        keystore.unlock("test-passphrase").unwrap();

        // Generate and save PQ KEM
        let keypair = keystore.load_or_generate_pq_kem().unwrap();
        assert!(keystore.encrypted_pq_kem_exists());

        // Load and verify
        let keypair2 = keystore.load_pq_kem().unwrap();
        assert_eq!(
            keypair.encapsulation_key_bytes(),
            keypair2.encapsulation_key_bytes()
        );

        // Test encapsulation/decapsulation
        let (ciphertext, secret1) = keypair.encapsulation_key().encapsulate();
        let secret2 = keypair2.decapsulate(&ciphertext).unwrap();
        assert_eq!(secret1, secret2);
    }

    #[test]
    fn test_encrypted_keystore_all_keys() {
        let temp_dir = TempDir::new().unwrap();
        let mut keystore = EncryptedKeystore::new(temp_dir.path());
        keystore.unlock("test-passphrase").unwrap();

        // Generate all key types
        let iroh_key = keystore.load_or_generate_iroh().unwrap();
        let pq_identity = keystore.load_or_generate_pq_identity().unwrap();
        let pq_kem = keystore.load_or_generate_pq_kem().unwrap();

        // Verify all exist
        assert!(keystore.encrypted_iroh_key_exists());
        assert!(keystore.encrypted_pq_identity_exists());
        assert!(keystore.encrypted_pq_kem_exists());

        // Lock and unlock
        keystore.lock();
        assert!(keystore.is_locked());

        keystore.unlock("test-passphrase").unwrap();
        assert!(!keystore.is_locked());

        // Reload and verify
        let iroh_key2 = keystore.load_iroh().unwrap();
        let pq_identity2 = keystore.load_pq_identity().unwrap();
        let pq_kem2 = keystore.load_pq_kem().unwrap();

        assert_eq!(iroh_key.public(), iroh_key2.public());
        assert_eq!(
            pq_identity.verifying_key_bytes(),
            pq_identity2.verifying_key_bytes()
        );
        assert_eq!(
            pq_kem.encapsulation_key_bytes(),
            pq_kem2.encapsulation_key_bytes()
        );
    }

    #[test]
    fn test_encrypted_keystore_persistence() {
        let temp_dir = TempDir::new().unwrap();
        let passphrase = "my-secure-passphrase";

        let iroh_public;
        let pq_short_id;

        // Create keystore and save keys
        {
            let mut keystore = EncryptedKeystore::new(temp_dir.path());
            keystore.unlock(passphrase).unwrap();

            let iroh_key = keystore.load_or_generate_iroh().unwrap();
            let pq_identity = keystore.load_or_generate_pq_identity().unwrap();

            iroh_public = iroh_key.public();
            pq_short_id = pq_identity.verifying_key().short_id();
        }

        // Create new keystore instance and load keys
        {
            let mut keystore = EncryptedKeystore::new(temp_dir.path());
            keystore.unlock(passphrase).unwrap();

            let iroh_key = keystore.load_iroh().unwrap();
            let pq_identity = keystore.load_pq_identity().unwrap();

            assert_eq!(iroh_key.public(), iroh_public);
            assert_eq!(pq_identity.verifying_key().short_id(), pq_short_id);
        }
    }

    #[test]
    fn test_encrypted_keystore_unencrypted_mode() {
        let temp_dir = TempDir::new().unwrap();
        let keystore = EncryptedKeystore::new_unencrypted(temp_dir.path());

        assert!(!keystore.is_locked());
        assert!(!keystore.is_encrypted());

        // Should work without unlock
        let key = keystore.load_or_generate_iroh().unwrap();

        // Keys should be stored in plaintext
        assert!(keystore.inner.exists());
        assert!(!keystore.encrypted_iroh_key_exists());

        let key2 = keystore.load_iroh().unwrap();
        assert_eq!(key.public(), key2.public());
    }

    #[test]
    fn test_encrypted_keystore_empty_passphrase_fails() {
        let temp_dir = TempDir::new().unwrap();
        let mut keystore = EncryptedKeystore::new(temp_dir.path());

        let result = keystore.unlock("");
        assert!(result.is_err());
    }

    #[test]
    fn test_encrypted_keystore_change_passphrase() {
        let temp_dir = TempDir::new().unwrap();
        let old_passphrase = "old-passphrase";
        let new_passphrase = "new-passphrase";

        let iroh_public;

        // Create keystore with old passphrase
        {
            let mut keystore = EncryptedKeystore::new(temp_dir.path());
            keystore.unlock(old_passphrase).unwrap();
            let key = keystore.load_or_generate_iroh().unwrap();
            iroh_public = key.public();

            // Change passphrase
            keystore.change_passphrase(new_passphrase).unwrap();
        }

        // Verify old passphrase no longer works
        {
            let mut keystore = EncryptedKeystore::new(temp_dir.path());
            keystore.unlock(old_passphrase).unwrap();
            let result = keystore.load_iroh();
            assert!(result.is_err());
        }

        // Verify new passphrase works
        {
            let mut keystore = EncryptedKeystore::new(temp_dir.path());
            keystore.unlock(new_passphrase).unwrap();
            let key = keystore.load_iroh().unwrap();
            assert_eq!(key.public(), iroh_public);
        }
    }

    #[test]
    fn test_encrypted_keystore_migration() {
        let temp_dir = TempDir::new().unwrap();
        let passphrase = "migration-passphrase";

        let iroh_public;

        // Create plaintext keystore
        {
            let keystore = Keystore::new(temp_dir.path());
            let key = keystore.load_or_generate_iroh().unwrap();
            iroh_public = key.public();
            assert!(keystore.exists());
        }

        // Migrate to encrypted
        {
            let mut keystore = EncryptedKeystore::new(temp_dir.path());
            keystore.migrate_to_encrypted(passphrase).unwrap();

            // Verify encrypted files exist
            assert!(keystore.encrypted_iroh_key_exists());

            // Verify plaintext file is removed
            assert!(!keystore.inner.exists());

            // Verify key is preserved
            let key = keystore.load_iroh().unwrap();
            assert_eq!(key.public(), iroh_public);
        }

        // Verify with fresh instance
        {
            let mut keystore = EncryptedKeystore::new(temp_dir.path());
            keystore.unlock(passphrase).unwrap();
            let key = keystore.load_iroh().unwrap();
            assert_eq!(key.public(), iroh_public);
        }
    }
}
