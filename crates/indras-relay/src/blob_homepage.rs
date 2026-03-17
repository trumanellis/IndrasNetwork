//! BlobStore-backed homepage persistence for relay-hosted serving.
//!
//! Implements [`HomepageStore`] using the relay's [`BlobStore`] so that
//! profile snapshots survive relay restarts and can be served when the
//! steward is offline. All events are stored in the Self tier and pinned.

use std::sync::Arc;

use indras_core::{EventId, InterfaceId};
use indras_homepage::{HomepageError, HomepageStore, ProfileFieldArtifact};
use indras_transport::protocol::{StorageTier, StoredEvent};

use crate::blob_store::BlobStore;

/// Profile payload version byte.
const PROFILE_VERSION: u8 = 1;

/// Well-known EventId for the profile snapshot (overwrite semantics).
fn profile_event_id() -> EventId {
    EventId { sender_hash: 0, sequence: 1 }
}

/// Derive a deterministic InterfaceId for a steward's homepage.
///
/// Uses `blake3("indras:homepage:profile:{steward_hex}")`.
fn homepage_interface_id(steward: &[u8; 32]) -> InterfaceId {
    let input = format!("indras:homepage:profile:{}", hex::encode(steward));
    InterfaceId::new(*blake3::hash(input.as_bytes()).as_bytes())
}

/// Derive an EventId for an artifact from its 32-byte ID.
fn artifact_event_id(artifact_id: &[u8; 32]) -> EventId {
    let sender_hash = u64::from_be_bytes(artifact_id[..8].try_into().unwrap());
    EventId { sender_hash, sequence: 0 }
}

/// [`HomepageStore`] implementation backed by [`BlobStore`].
///
/// Stores profile snapshots and artifacts as pinned events in the Self tier.
pub struct BlobStoreHomepageStore {
    store: Arc<BlobStore>,
    interface_id: InterfaceId,
}

impl BlobStoreHomepageStore {
    /// Create a new BlobStore-backed homepage store.
    pub fn new(store: Arc<BlobStore>, steward: &[u8; 32]) -> Self {
        Self {
            store,
            interface_id: homepage_interface_id(steward),
        }
    }
}

impl HomepageStore for BlobStoreHomepageStore {
    fn load_profile(&self) -> Result<Vec<ProfileFieldArtifact>, HomepageError> {
        let event_id = profile_event_id();
        match self.store.get_event(StorageTier::Self_, &self.interface_id, &event_id) {
            Ok(Some(event)) => {
                let data = &event.encrypted_event;
                if data.is_empty() {
                    return Ok(Vec::new());
                }
                let version = data[0];
                match version {
                    PROFILE_VERSION => serde_json::from_slice(&data[1..])
                        .map_err(|e| HomepageError::Storage(format!("Failed to parse profile: {e}"))),
                    _ => Err(HomepageError::Storage(format!("Unknown profile version: {version}"))),
                }
            }
            Ok(None) => Ok(Vec::new()),
            Err(e) => Err(HomepageError::Storage(format!("Failed to load profile: {e}"))),
        }
    }

    fn save_profile(&self, fields: &[ProfileFieldArtifact]) -> Result<(), HomepageError> {
        let json = serde_json::to_vec(fields)
            .map_err(|e| HomepageError::Storage(format!("Failed to serialize profile: {e}")))?;
        let mut data = Vec::with_capacity(1 + json.len());
        data.push(PROFILE_VERSION);
        data.extend_from_slice(&json);

        let event = StoredEvent::new(profile_event_id(), data, [0u8; 12]);
        self.store
            .store_event_tiered(StorageTier::Self_, self.interface_id, &event)
            .map_err(|e| HomepageError::Storage(format!("Failed to store profile: {e}")))?;

        // Pin so it survives cleanup
        self.store
            .pin_event(StorageTier::Self_, &self.interface_id, &profile_event_id())
            .map_err(|e| HomepageError::Storage(format!("Failed to pin profile: {e}")))?;

        Ok(())
    }

    fn load_artifact(&self, id: &[u8; 32]) -> Result<Vec<u8>, HomepageError> {
        let event_id = artifact_event_id(id);
        match self.store.get_event(StorageTier::Self_, &self.interface_id, &event_id) {
            Ok(Some(event)) => Ok(event.encrypted_event),
            Ok(None) => Err(HomepageError::Storage("Artifact not found".to_string())),
            Err(e) => Err(HomepageError::Storage(format!("Failed to load artifact: {e}"))),
        }
    }

    fn save_artifact(&self, id: &[u8; 32], data: &[u8]) -> Result<(), HomepageError> {
        let event_id = artifact_event_id(id);
        let event = StoredEvent::new(event_id, data.to_vec(), [0u8; 12]);
        self.store
            .store_event_tiered(StorageTier::Self_, self.interface_id, &event)
            .map_err(|e| HomepageError::Storage(format!("Failed to store artifact: {e}")))?;

        // Pin artifacts
        self.store
            .pin_event(StorageTier::Self_, &self.interface_id, &event_id)
            .map_err(|e| HomepageError::Storage(format!("Failed to pin artifact: {e}")))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indras_homepage::ProfileFieldArtifact;
    use tempfile::TempDir;

    fn test_store(steward: &[u8; 32]) -> (BlobStoreHomepageStore, TempDir) {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("test-homepage.redb");
        let bs = BlobStore::open(&db_path).unwrap();
        let store = BlobStoreHomepageStore::new(Arc::new(bs), steward);
        (store, dir)
    }

    #[test]
    fn round_trip_profile() {
        let steward = [0x42u8; 32];
        let (store, _dir) = test_store(&steward);

        // Empty initially
        let loaded = store.load_profile().unwrap();
        assert!(loaded.is_empty());

        // Save and reload
        let fields = vec![ProfileFieldArtifact {
            field_name: "display_name".to_string(),
            display_value: "Alice".to_string(),
            grants: Vec::new(),
        }];
        store.save_profile(&fields).unwrap();

        let loaded = store.load_profile().unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].field_name, "display_name");
        assert_eq!(loaded[0].display_value, "Alice");
    }

    #[test]
    fn round_trip_artifact() {
        let steward = [0x42u8; 32];
        let (store, _dir) = test_store(&steward);

        let id = [0xAAu8; 32];
        let data = b"hello world";
        store.save_artifact(&id, data).unwrap();

        let loaded = store.load_artifact(&id).unwrap();
        assert_eq!(loaded, data);
    }

    #[test]
    fn profile_overwrite() {
        let steward = [0x42u8; 32];
        let (store, _dir) = test_store(&steward);

        let fields1 = vec![ProfileFieldArtifact {
            field_name: "display_name".to_string(),
            display_value: "Alice".to_string(),
            grants: Vec::new(),
        }];
        store.save_profile(&fields1).unwrap();

        let fields2 = vec![ProfileFieldArtifact {
            field_name: "display_name".to_string(),
            display_value: "Bob".to_string(),
            grants: Vec::new(),
        }];
        store.save_profile(&fields2).unwrap();

        let loaded = store.load_profile().unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].display_value, "Bob");
    }
}
