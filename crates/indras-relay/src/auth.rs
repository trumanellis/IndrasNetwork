//! Authentication service for relay credential validation
//!
//! Validates signed credentials that link a transport-layer Ed25519 identity
//! (iroh `PublicKey`) to a profile-layer `PlayerId`. The credential format
//! is a thin interface — when `indras-profile` ships, we swap the verification
//! logic without changing the relay's tier model.
//!
//! ## Credential Format (v1 — simple signed token)
//!
//! ```text
//! { player_id: [u8; 32], transport_pubkey: [u8; 32], expires_at_millis: i64 }
//! ```
//!
//! The credential is signed with the player's Ed25519 signing key (from profile).
//! The relay verifies: (1) signature, (2) expiry, (3) transport_pubkey matches
//! the connecting peer's iroh identity.

use dashmap::DashMap;
use tracing::debug;

use indras_core::identity::PeerIdentity;
use indras_crypto::credential;
use indras_transport::identity::IrohIdentity;
use indras_transport::protocol::StorageTier;

use crate::config::RelayConfig;
use crate::error::{RelayError, RelayResult};
use crate::tier;

/// An authenticated session for a connected peer
#[derive(Debug, Clone)]
pub struct AuthSession {
    /// The authenticated player ID
    pub player_id: [u8; 32],
    /// The transport identity
    pub transport_id: IrohIdentity,
    /// The highest tier this peer has access to
    pub highest_tier: StorageTier,
    /// All granted tiers
    pub granted_tiers: Vec<StorageTier>,
}

/// Authentication service that validates credentials and tracks sessions
pub struct AuthService {
    /// Owner's player ID (None for community mode)
    owner_player_id: Option<[u8; 32]>,
    /// Owner's contacts (player IDs that get Connections tier)
    contacts: DashMap<[u8; 32], ()>,
    /// Active authenticated sessions: transport identity → session
    sessions: DashMap<IrohIdentity, AuthSession>,
}

impl AuthService {
    /// Create a new auth service from relay config
    pub fn new(config: &RelayConfig) -> Self {
        let owner_player_id = config.owner_player_id.as_ref().and_then(|hex_str| {
            parse_hex_32(hex_str)
        });

        Self {
            owner_player_id,
            contacts: DashMap::new(),
            sessions: DashMap::new(),
        }
    }

    /// Validate a credential and create an authenticated session.
    ///
    /// Returns the session on success, or an error if validation fails.
    pub fn authenticate(
        &self,
        transport_id: &IrohIdentity,
        credential_bytes: &[u8],
        player_id: &[u8; 32],
    ) -> RelayResult<AuthSession> {
        // Parse the signed credential
        let signed = credential::parse_credential(credential_bytes)
            .map_err(|e| RelayError::InvalidCredential(e.to_string()))?;

        // Verify player_id matches
        if &signed.credential.player_id != player_id {
            return Err(RelayError::InvalidCredential(
                "player_id mismatch between message and credential".into(),
            ));
        }

        // Verify transport key matches the connecting peer
        let transport_bytes = transport_id.as_bytes();
        if signed.credential.transport_pubkey != transport_bytes.as_slice() {
            return Err(RelayError::InvalidCredential(
                "transport_pubkey does not match connecting peer".into(),
            ));
        }

        // Verify expiry
        let now = chrono::Utc::now().timestamp_millis();
        if signed.credential.expires_at_millis < now {
            return Err(RelayError::InvalidCredential(
                "credential has expired".into(),
            ));
        }

        // Verify Ed25519 signature
        credential::verify_credential(&signed)
            .map_err(|e| RelayError::AuthenticationFailed(e.to_string()))?;

        // Determine tier access
        let contacts: Vec<[u8; 32]> = self.contacts.iter().map(|e| *e.key()).collect();
        let highest_tier = tier::determine_tier(
            player_id,
            self.owner_player_id.as_ref(),
            &contacts,
        );
        let granted_tiers = tier::granted_tiers(highest_tier);

        let session = AuthSession {
            player_id: *player_id,
            transport_id: *transport_id,
            highest_tier,
            granted_tiers: granted_tiers.clone(),
        };

        // Store the session
        self.sessions.insert(*transport_id, session.clone());

        debug!(
            player_id = %short_hex(player_id),
            tier = ?highest_tier,
            "Peer authenticated"
        );

        Ok(session)
    }

    /// Check if a transport identity has an active authenticated session
    pub fn get_session(&self, transport_id: &IrohIdentity) -> Option<AuthSession> {
        self.sessions.get(transport_id).map(|s| s.clone())
    }

    /// Check if a peer has access to a specific tier
    pub fn has_tier_access(&self, transport_id: &IrohIdentity, tier: StorageTier) -> bool {
        self.sessions
            .get(transport_id)
            .map(|s| s.granted_tiers.contains(&tier))
            .unwrap_or(false)
    }

    /// Remove an authenticated session (on disconnect)
    pub fn remove_session(&self, transport_id: &IrohIdentity) {
        self.sessions.remove(transport_id);
    }

    /// Add a contact (grants Connections tier access)
    pub fn add_contact(&self, player_id: [u8; 32]) {
        self.contacts.insert(player_id, ());
    }

    /// Remove a contact
    pub fn remove_contact(&self, player_id: &[u8; 32]) {
        self.contacts.remove(player_id);
    }

    /// Get the number of active sessions
    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }

    /// Get the owner's player ID
    pub fn owner_player_id(&self) -> Option<&[u8; 32]> {
        self.owner_player_id.as_ref()
    }

    /// Replace the contacts list with a new set of player IDs.
    ///
    /// Called when the owner syncs their profile artifact grants.
    pub fn sync_contacts(&self, contacts: Vec<[u8; 32]>) {
        self.contacts.clear();
        for id in contacts {
            self.contacts.insert(id, ());
        }
    }

    /// Get the current number of contacts.
    pub fn contact_count(&self) -> usize {
        self.contacts.len()
    }

    /// Get all contact IDs as a list
    pub fn contact_ids(&self) -> Vec<[u8; 32]> {
        self.contacts.iter().map(|e| *e.key()).collect()
    }

    /// Load contacts from a JSON file.
    ///
    /// The file contains a JSON array of hex-encoded 32-byte player IDs.
    pub fn load_contacts(&self, path: &std::path::Path) -> Result<(), RelayError> {
        if !path.exists() {
            return Ok(());
        }
        let data = std::fs::read_to_string(path)?;
        let hex_ids: Vec<String> = serde_json::from_str(&data).map_err(|e| {
            RelayError::Config(format!("Failed to parse contacts file: {e}"))
        })?;

        self.contacts.clear();
        for hex in hex_ids {
            if let Some(id) = parse_hex_32(&hex) {
                self.contacts.insert(id, ());
            }
        }
        Ok(())
    }

    /// Save contacts to a JSON file.
    pub fn save_contacts(&self, path: &std::path::Path) -> Result<(), RelayError> {
        let hex_ids: Vec<String> = self.contacts
            .iter()
            .map(|e| e.key().iter().map(|b| format!("{b:02x}")).collect())
            .collect();
        let data = serde_json::to_string_pretty(&hex_ids).map_err(|e| {
            RelayError::Config(format!("Failed to serialize contacts: {e}"))
        })?;
        std::fs::write(path, data)?;
        Ok(())
    }
}

/// Parse a 64-character hex string into a 32-byte array
fn parse_hex_32(hex: &str) -> Option<[u8; 32]> {
    if hex.len() != 64 {
        return None;
    }
    let mut bytes = [0u8; 32];
    for i in 0..32 {
        bytes[i] = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(bytes)
}

/// Short hex display
fn short_hex(bytes: &[u8]) -> String {
    if bytes.len() >= 4 {
        format!(
            "{:02x}{:02x}..{:02x}{:02x}",
            bytes[0], bytes[1],
            bytes[bytes.len() - 2], bytes[bytes.len() - 1]
        )
    } else {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;
    use iroh::SecretKey;
    use rand::Rng;

    /// Generate a random Ed25519 signing key using rand 0.9
    fn random_signing_key() -> SigningKey {
        let mut bytes = [0u8; 32];
        rand::rng().fill(&mut bytes);
        SigningKey::from_bytes(&bytes)
    }

    fn make_credential(
        signing_key: &SigningKey,
        transport_pubkey: [u8; 32],
        expires_at_millis: i64,
    ) -> Vec<u8> {
        indras_crypto::credential::create_credential(signing_key, transport_pubkey, expires_at_millis)
    }

    fn test_config() -> RelayConfig {
        RelayConfig::default()
    }

    #[test]
    fn test_valid_credential() {
        let signing_key = random_signing_key();
        let player_id = signing_key.verifying_key().to_bytes();

        let transport_secret = SecretKey::generate(&mut rand::rng());
        let transport_id = IrohIdentity::new(transport_secret.public());
        let transport_bytes = transport_id.as_bytes();
        let mut transport_arr = [0u8; 32];
        transport_arr.copy_from_slice(&transport_bytes);

        let expires = chrono::Utc::now().timestamp_millis() + 3_600_000; // 1 hour
        let cred_bytes = make_credential(&signing_key, transport_arr, expires);

        let auth = AuthService::new(&test_config());
        let session = auth.authenticate(&transport_id, &cred_bytes, &player_id).unwrap();

        assert_eq!(session.player_id, player_id);
        assert_eq!(session.highest_tier, StorageTier::Public);
    }

    #[test]
    fn test_expired_credential() {
        let signing_key = random_signing_key();
        let player_id = signing_key.verifying_key().to_bytes();

        let transport_secret = SecretKey::generate(&mut rand::rng());
        let transport_id = IrohIdentity::new(transport_secret.public());
        let mut transport_arr = [0u8; 32];
        transport_arr.copy_from_slice(&transport_id.as_bytes());

        let expires = chrono::Utc::now().timestamp_millis() - 1000; // Already expired
        let cred_bytes = make_credential(&signing_key, transport_arr, expires);

        let auth = AuthService::new(&test_config());
        let result = auth.authenticate(&transport_id, &cred_bytes, &player_id);
        assert!(result.is_err());
    }

    #[test]
    fn test_wrong_transport_key() {
        let signing_key = random_signing_key();
        let player_id = signing_key.verifying_key().to_bytes();

        let transport_secret = SecretKey::generate(&mut rand::rng());
        let transport_id = IrohIdentity::new(transport_secret.public());

        let wrong_transport = [0xFFu8; 32]; // Wrong key
        let expires = chrono::Utc::now().timestamp_millis() + 3_600_000;
        let cred_bytes = make_credential(&signing_key, wrong_transport, expires);

        let auth = AuthService::new(&test_config());
        let result = auth.authenticate(&transport_id, &cred_bytes, &player_id);
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_signature() {
        let signing_key = random_signing_key();
        let player_id = signing_key.verifying_key().to_bytes();

        let transport_secret = SecretKey::generate(&mut rand::rng());
        let transport_id = IrohIdentity::new(transport_secret.public());
        let mut transport_arr = [0u8; 32];
        transport_arr.copy_from_slice(&transport_id.as_bytes());

        let expires = chrono::Utc::now().timestamp_millis() + 3_600_000;
        let mut cred_bytes = make_credential(&signing_key, transport_arr, expires);

        // Corrupt the signature
        let len = cred_bytes.len();
        cred_bytes[len - 1] ^= 0xFF;

        let auth = AuthService::new(&test_config());
        let result = auth.authenticate(&transport_id, &cred_bytes, &player_id);
        assert!(result.is_err());
    }

    #[test]
    fn test_owner_gets_self_tier() {
        let signing_key = random_signing_key();
        let player_id = signing_key.verifying_key().to_bytes();

        let transport_secret = SecretKey::generate(&mut rand::rng());
        let transport_id = IrohIdentity::new(transport_secret.public());
        let mut transport_arr = [0u8; 32];
        transport_arr.copy_from_slice(&transport_id.as_bytes());

        let expires = chrono::Utc::now().timestamp_millis() + 3_600_000;
        let cred_bytes = make_credential(&signing_key, transport_arr, expires);

        // Create config with this player as owner
        let mut config = test_config();
        let hex: String = player_id.iter().map(|b| format!("{b:02x}")).collect();
        config.owner_player_id = Some(hex);

        let auth = AuthService::new(&config);
        let session = auth.authenticate(&transport_id, &cred_bytes, &player_id).unwrap();

        assert_eq!(session.highest_tier, StorageTier::Self_);
        assert_eq!(session.granted_tiers.len(), 3);
    }

    #[test]
    fn test_contact_gets_connections_tier() {
        let signing_key = random_signing_key();
        let player_id = signing_key.verifying_key().to_bytes();

        let transport_secret = SecretKey::generate(&mut rand::rng());
        let transport_id = IrohIdentity::new(transport_secret.public());
        let mut transport_arr = [0u8; 32];
        transport_arr.copy_from_slice(&transport_id.as_bytes());

        let expires = chrono::Utc::now().timestamp_millis() + 3_600_000;
        let cred_bytes = make_credential(&signing_key, transport_arr, expires);

        let auth = AuthService::new(&test_config());
        auth.add_contact(player_id);

        let session = auth.authenticate(&transport_id, &cred_bytes, &player_id).unwrap();
        assert_eq!(session.highest_tier, StorageTier::Connections);
        assert_eq!(session.granted_tiers.len(), 2);
    }

    #[test]
    fn test_session_tracking() {
        let signing_key = random_signing_key();
        let player_id = signing_key.verifying_key().to_bytes();

        let transport_secret = SecretKey::generate(&mut rand::rng());
        let transport_id = IrohIdentity::new(transport_secret.public());
        let mut transport_arr = [0u8; 32];
        transport_arr.copy_from_slice(&transport_id.as_bytes());

        let expires = chrono::Utc::now().timestamp_millis() + 3_600_000;
        let cred_bytes = make_credential(&signing_key, transport_arr, expires);

        let auth = AuthService::new(&test_config());
        auth.authenticate(&transport_id, &cred_bytes, &player_id).unwrap();

        assert_eq!(auth.session_count(), 1);
        assert!(auth.get_session(&transport_id).is_some());
        assert!(auth.has_tier_access(&transport_id, StorageTier::Public));

        auth.remove_session(&transport_id);
        assert_eq!(auth.session_count(), 0);
        assert!(auth.get_session(&transport_id).is_none());
    }

    #[test]
    fn test_parse_hex_32() {
        let hex = "abcd1234abcd1234abcd1234abcd1234abcd1234abcd1234abcd1234abcd1234";
        let result = parse_hex_32(hex).unwrap();
        assert_eq!(result[0], 0xab);
        assert_eq!(result[1], 0xcd);
        assert_eq!(result[31], 0x34);

        assert!(parse_hex_32("too_short").is_none());
    }

    #[test]
    fn test_sync_contacts() {
        let auth = AuthService::new(&test_config());
        assert_eq!(auth.contact_count(), 0);

        let contacts = vec![[0x01u8; 32], [0x02u8; 32], [0x03u8; 32]];
        auth.sync_contacts(contacts);
        assert_eq!(auth.contact_count(), 3);

        // Sync with fewer contacts replaces the list
        auth.sync_contacts(vec![[0x04u8; 32]]);
        assert_eq!(auth.contact_count(), 1);
    }

    #[test]
    fn test_save_and_load_contacts() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("contacts.json");

        let auth = AuthService::new(&test_config());
        auth.add_contact([0xAAu8; 32]);
        auth.add_contact([0xBBu8; 32]);

        auth.save_contacts(&path).unwrap();

        let auth2 = AuthService::new(&test_config());
        auth2.load_contacts(&path).unwrap();
        assert_eq!(auth2.contact_count(), 2);
    }

    #[test]
    fn test_load_contacts_missing_file() {
        let auth = AuthService::new(&test_config());
        let path = std::path::Path::new("/tmp/nonexistent_contacts_file.json");
        // Should succeed silently when file doesn't exist
        assert!(auth.load_contacts(path).is_ok());
        assert_eq!(auth.contact_count(), 0);
    }
}
