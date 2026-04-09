//! Persistent storage for DTN bundles
//!
//! Stores [`Bundle<IrohIdentity>`] in a redb database for offline peer delivery.
//! Bundles survive node restarts, ensuring messages to offline peers are never lost.
//!
//! ## Tables
//!
//! - `dtn_bundles`: `bundle_key (24 bytes) → serialized Bundle`
//! - `dtn_pending`: `destination_hash (8 bytes) ++ bundle_key (24 bytes) → ()`
//!
//! The pending table is a secondary index for efficient `pending_for()` lookups.

use std::path::Path;
use std::sync::Arc;

use redb::{Database, ReadableTable, ReadableTableMetadata, TableDefinition};
use tracing::{debug, warn};

use indras_core::PeerIdentity;
use indras_dtn::{Bundle, BundleId};
use indras_transport::IrohIdentity;

use crate::error::{NodeError, NodeResult};

/// Table: bundle_key (source_hash:8 + creation_ts:8 + sequence:8 = 24 bytes) → serialized Bundle
const BUNDLES_TABLE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("dtn_bundles");

/// Table: destination_hash (8 bytes) ++ bundle_key (24 bytes) = 32 bytes → empty value
/// Secondary index for efficient pending_for() lookups by destination.
const PENDING_TABLE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("dtn_pending");

/// Encode a BundleId into a 20-byte key (8 + 8 + 4)
fn bundle_id_to_key(id: &BundleId) -> [u8; 20] {
    let mut key = [0u8; 20];
    key[0..8].copy_from_slice(&id.source_hash.to_be_bytes());
    key[8..16].copy_from_slice(&id.creation_timestamp.to_be_bytes());
    key[16..20].copy_from_slice(&id.sequence.to_be_bytes());
    key
}

/// Build the pending index key: destination_hash (8) ++ bundle_key (20) = 28 bytes
fn pending_key(destination: &IrohIdentity, bundle_id: &BundleId) -> [u8; 28] {
    let dest_hash = identity_hash(destination);
    let bundle_key = bundle_id_to_key(bundle_id);
    let mut key = [0u8; 28];
    key[0..8].copy_from_slice(&dest_hash.to_be_bytes());
    key[8..28].copy_from_slice(&bundle_key);
    key
}

/// Hash an identity for index keys (consistent with Packet::hash_identity)
fn identity_hash(identity: &IrohIdentity) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    identity.hash(&mut hasher);
    hasher.finish()
}

/// Persistent storage for DTN bundles backed by redb
pub struct BundleStore {
    db: Arc<Database>,
}

impl BundleStore {
    /// Create or open a bundle store at the given path
    pub fn open(path: &Path) -> NodeResult<Self> {
        std::fs::create_dir_all(path.parent().unwrap_or(path)).map_err(|e| {
            NodeError::Io(format!("Failed to create DTN database directory: {e}"))
        })?;

        let db = Database::create(path).map_err(|e| {
            NodeError::Io(format!("Failed to open DTN database: {e}"))
        })?;

        // Initialize tables
        let txn = db.begin_write().map_err(|e| {
            NodeError::Io(format!("Failed to begin DTN table init: {e}"))
        })?;
        {
            let _ = txn.open_table(BUNDLES_TABLE).map_err(|e| {
                NodeError::Io(format!("Failed to create dtn_bundles table: {e}"))
            })?;
            let _ = txn.open_table(PENDING_TABLE).map_err(|e| {
                NodeError::Io(format!("Failed to create dtn_pending table: {e}"))
            })?;
        }
        txn.commit().map_err(|e| {
            NodeError::Io(format!("Failed to commit DTN table init: {e}"))
        })?;

        Ok(Self { db: Arc::new(db) })
    }

    /// Store a bundle for offline delivery
    pub fn store_bundle(&self, bundle: &Bundle<IrohIdentity>) -> NodeResult<()> {
        let bundle_key = bundle_id_to_key(&bundle.bundle_id);
        let bundle_bytes = postcard::to_allocvec(bundle).map_err(|e| {
            NodeError::Io(format!("Failed to serialize bundle: {e}"))
        })?;
        let pend_key = pending_key(&bundle.packet.destination, &bundle.bundle_id);

        let txn = self.db.begin_write().map_err(|e| {
            NodeError::Io(format!("Failed to begin write: {e}"))
        })?;
        {
            let mut bundles = txn.open_table(BUNDLES_TABLE).map_err(|e| {
                NodeError::Io(format!("Failed to open bundles table: {e}"))
            })?;
            bundles
                .insert(bundle_key.as_slice(), bundle_bytes.as_slice())
                .map_err(|e| NodeError::Io(format!("Failed to insert bundle: {e}")))?;

            let mut pending = txn.open_table(PENDING_TABLE).map_err(|e| {
                NodeError::Io(format!("Failed to open pending table: {e}"))
            })?;
            pending
                .insert(pend_key.as_slice(), &[] as &[u8])
                .map_err(|e| NodeError::Io(format!("Failed to insert pending: {e}")))?;
        }
        txn.commit().map_err(|e| {
            NodeError::Io(format!("Failed to commit bundle store: {e}"))
        })?;

        debug!(
            bundle_id = %bundle.bundle_id,
            destination = %bundle.packet.destination.short_id(),
            "Stored DTN bundle"
        );
        Ok(())
    }

    /// Retrieve a bundle by its ID
    pub fn get_bundle(&self, id: &BundleId) -> NodeResult<Option<Bundle<IrohIdentity>>> {
        let bundle_key = bundle_id_to_key(id);

        let txn = self.db.begin_read().map_err(|e| {
            NodeError::Io(format!("Failed to begin read: {e}"))
        })?;
        let table = txn.open_table(BUNDLES_TABLE).map_err(|e| {
            NodeError::Io(format!("Failed to open bundles table: {e}"))
        })?;

        match table.get(bundle_key.as_slice()) {
            Ok(Some(value)) => {
                let bundle: Bundle<IrohIdentity> =
                    postcard::from_bytes(value.value()).map_err(|e| {
                        NodeError::Io(format!("Failed to deserialize bundle: {e}"))
                    })?;
                Ok(Some(bundle))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(NodeError::Io(format!("Failed to get bundle: {e}"))),
        }
    }

    /// Get all pending bundles for a destination peer
    pub fn pending_for(
        &self,
        destination: &IrohIdentity,
    ) -> NodeResult<Vec<Bundle<IrohIdentity>>> {
        let dest_hash = identity_hash(destination);
        let prefix_start = {
            let mut k = [0u8; 28];
            k[0..8].copy_from_slice(&dest_hash.to_be_bytes());
            k
        };
        let prefix_end = {
            let mut k = [0u8; 28];
            k[0..8].copy_from_slice(&dest_hash.to_be_bytes());
            // Fill remaining 20 bytes with 0xFF for range end
            k[8..].fill(0xFF);
            k
        };

        let txn = self.db.begin_read().map_err(|e| {
            NodeError::Io(format!("Failed to begin read: {e}"))
        })?;
        let pending = txn.open_table(PENDING_TABLE).map_err(|e| {
            NodeError::Io(format!("Failed to open pending table: {e}"))
        })?;
        let bundles_table = txn.open_table(BUNDLES_TABLE).map_err(|e| {
            NodeError::Io(format!("Failed to open bundles table: {e}"))
        })?;

        let mut bundles = Vec::new();
        let range = pending
            .range(prefix_start.as_slice()..=prefix_end.as_slice())
            .map_err(|e| NodeError::Io(format!("Failed to range scan pending: {e}")))?;

        for entry in range {
            let (key, _) = entry.map_err(|e| {
                NodeError::Io(format!("Failed to read pending entry: {e}"))
            })?;
            let key_bytes = key.value();
            // Extract the bundle_key from bytes [8..32]
            let bundle_key = &key_bytes[8..28];

            match bundles_table.get(bundle_key) {
                Ok(Some(value)) => {
                    match postcard::from_bytes::<Bundle<IrohIdentity>>(value.value()) {
                        Ok(bundle) => bundles.push(bundle),
                        Err(e) => {
                            warn!("Skipping corrupt bundle: {e}");
                        }
                    }
                }
                Ok(None) => {
                    warn!("Pending index references missing bundle");
                }
                Err(e) => {
                    warn!("Failed to read bundle: {e}");
                }
            }
        }

        Ok(bundles)
    }

    /// Delete a bundle (after successful delivery or expiration)
    pub fn delete_bundle(
        &self,
        id: &BundleId,
        destination: &IrohIdentity,
    ) -> NodeResult<()> {
        let bundle_key = bundle_id_to_key(id);
        let pend_key = pending_key(destination, id);

        let txn = self.db.begin_write().map_err(|e| {
            NodeError::Io(format!("Failed to begin write: {e}"))
        })?;
        {
            let mut bundles = txn.open_table(BUNDLES_TABLE).map_err(|e| {
                NodeError::Io(format!("Failed to open bundles table: {e}"))
            })?;
            bundles
                .remove(bundle_key.as_slice())
                .map_err(|e| NodeError::Io(format!("Failed to remove bundle: {e}")))?;

            let mut pending = txn.open_table(PENDING_TABLE).map_err(|e| {
                NodeError::Io(format!("Failed to open pending table: {e}"))
            })?;
            pending
                .remove(pend_key.as_slice())
                .map_err(|e| NodeError::Io(format!("Failed to remove pending: {e}")))?;
        }
        txn.commit().map_err(|e| {
            NodeError::Io(format!("Failed to commit bundle delete: {e}"))
        })?;

        debug!(bundle_id = %id, "Deleted DTN bundle");
        Ok(())
    }

    /// Get all stored bundles
    pub fn all_bundles(&self) -> NodeResult<Vec<Bundle<IrohIdentity>>> {
        let txn = self.db.begin_read().map_err(|e| {
            NodeError::Io(format!("Failed to begin read: {e}"))
        })?;
        let table = txn.open_table(BUNDLES_TABLE).map_err(|e| {
            NodeError::Io(format!("Failed to open bundles table: {e}"))
        })?;

        let mut bundles = Vec::new();
        let iter = table.iter().map_err(|e| {
            NodeError::Io(format!("Failed to iterate bundles: {e}"))
        })?;

        for entry in iter {
            let (_, value) = entry.map_err(|e| {
                NodeError::Io(format!("Failed to read bundle entry: {e}"))
            })?;
            match postcard::from_bytes::<Bundle<IrohIdentity>>(value.value()) {
                Ok(bundle) => bundles.push(bundle),
                Err(e) => warn!("Skipping corrupt bundle: {e}"),
            }
        }

        Ok(bundles)
    }

    /// Count stored bundles
    pub fn count(&self) -> NodeResult<usize> {
        let txn = self.db.begin_read().map_err(|e| {
            NodeError::Io(format!("Failed to begin read: {e}"))
        })?;
        let table = txn.open_table(BUNDLES_TABLE).map_err(|e| {
            NodeError::Io(format!("Failed to open bundles table: {e}"))
        })?;
        let len = table.len().map_err(|e| {
            NodeError::Io(format!("Failed to count bundles: {e}"))
        })?;
        Ok(len as usize)
    }

    /// Remove all expired bundles, returning the count removed
    pub fn cleanup_expired(&self) -> NodeResult<usize> {
        // First pass: find expired bundle IDs
        let expired: Vec<(BundleId, IrohIdentity)> = {
            let all = self.all_bundles()?;
            all.into_iter()
                .filter(|b| b.is_expired())
                .map(|b| (b.bundle_id, b.packet.destination.clone()))
                .collect()
        };

        let count = expired.len();
        for (id, dest) in &expired {
            self.delete_bundle(id, dest)?;
        }

        if count > 0 {
            debug!(count, "Cleaned up expired DTN bundles");
        }
        Ok(count)
    }

    /// Clear all bundles (for testing)
    pub fn clear(&self) -> NodeResult<()> {
        let txn = self.db.begin_write().map_err(|e| {
            NodeError::Io(format!("Failed to begin write: {e}"))
        })?;
        {
            let mut bundles = txn.open_table(BUNDLES_TABLE).map_err(|e| {
                NodeError::Io(format!("Failed to open bundles table: {e}"))
            })?;
            // Drain all entries
            while let Some(entry) = bundles.pop_last().map_err(|e| {
                NodeError::Io(format!("Failed to pop bundle: {e}"))
            })? {
                let _ = entry;
            }

            let mut pending = txn.open_table(PENDING_TABLE).map_err(|e| {
                NodeError::Io(format!("Failed to open pending table: {e}"))
            })?;
            while let Some(entry) = pending.pop_last().map_err(|e| {
                NodeError::Io(format!("Failed to pop pending: {e}"))
            })? {
                let _ = entry;
            }
        }
        txn.commit().map_err(|e| {
            NodeError::Io(format!("Failed to commit clear: {e}"))
        })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use indras_core::packet::{EncryptedPayload, Packet, PacketId};

    fn temp_store() -> (BundleStore, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let store = BundleStore::open(&dir.path().join("dtn.redb")).unwrap();
        (store, dir)
    }

    fn make_identity(key_byte: u8) -> IrohIdentity {
        // Create a deterministic identity from a byte
        let secret = iroh::SecretKey::from_bytes(&{
            let mut bytes = [0u8; 32];
            bytes[0] = key_byte;
            bytes
        });
        IrohIdentity::from(secret.public())
    }

    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_SEQ: AtomicU64 = AtomicU64::new(1);

    fn make_bundle(
        source: IrohIdentity,
        destination: IrohIdentity,
        payload: &[u8],
        lifetime_secs: u64,
    ) -> Bundle<IrohIdentity> {
        let seq = TEST_SEQ.fetch_add(1, Ordering::Relaxed);
        let packet = Packet::new(
            PacketId::new(identity_hash(&source), seq),
            source,
            destination,
            EncryptedPayload::plaintext(payload.to_vec()),
            vec![],
        );
        Bundle::from_packet(packet, chrono::Duration::seconds(lifetime_secs as i64))
    }

    #[test]
    fn test_store_and_retrieve() {
        let (store, _dir) = temp_store();
        let src = make_identity(1);
        let dst = make_identity(2);

        let bundle = make_bundle(src, dst, b"hello", 3600);
        let id = bundle.bundle_id;

        store.store_bundle(&bundle).unwrap();

        let retrieved = store.get_bundle(&id).unwrap().unwrap();
        assert_eq!(retrieved.bundle_id, id);
        assert_eq!(retrieved.packet.payload.as_bytes(), b"hello");
    }

    #[test]
    fn test_pending_for() {
        let (store, _dir) = temp_store();
        let src = make_identity(1);
        let dst_a = make_identity(2);
        let dst_b = make_identity(3);

        let bundle_a = make_bundle(src.clone(), dst_a.clone(), b"to A", 3600);
        let bundle_b = make_bundle(src.clone(), dst_b.clone(), b"to B", 3600);

        store.store_bundle(&bundle_a).unwrap();
        store.store_bundle(&bundle_b).unwrap();

        let pending_a = store.pending_for(&dst_a).unwrap();
        assert_eq!(pending_a.len(), 1);
        assert_eq!(pending_a[0].packet.payload.as_bytes(), b"to A");

        let pending_b = store.pending_for(&dst_b).unwrap();
        assert_eq!(pending_b.len(), 1);
        assert_eq!(pending_b[0].packet.payload.as_bytes(), b"to B");
    }

    #[test]
    fn test_delete_bundle() {
        let (store, _dir) = temp_store();
        let src = make_identity(1);
        let dst = make_identity(2);

        let bundle = make_bundle(src, dst.clone(), b"temp", 3600);
        let id = bundle.bundle_id;

        store.store_bundle(&bundle).unwrap();
        assert_eq!(store.count().unwrap(), 1);

        store.delete_bundle(&id, &dst).unwrap();
        assert_eq!(store.count().unwrap(), 0);
        assert!(store.get_bundle(&id).unwrap().is_none());
        assert!(store.pending_for(&dst).unwrap().is_empty());
    }

    #[test]
    fn test_clear() {
        let (store, _dir) = temp_store();
        let src = make_identity(1);
        let dst = make_identity(2);

        store
            .store_bundle(&make_bundle(src.clone(), dst.clone(), b"a", 3600))
            .unwrap();
        store
            .store_bundle(&make_bundle(src, dst, b"b", 3600))
            .unwrap();
        assert_eq!(store.count().unwrap(), 2);

        store.clear().unwrap();
        assert_eq!(store.count().unwrap(), 0);
    }
}
