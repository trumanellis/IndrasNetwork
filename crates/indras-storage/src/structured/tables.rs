//! redb table definitions and storage manager
//!
//! Defines all tables used for structured storage.

use std::path::PathBuf;
use std::sync::Arc;

use redb::{Database, TableDefinition};
use tracing::{debug, info, instrument};

use crate::error::StorageError;

/// Type alias for scan results to simplify complex type
pub type ScanResults = Vec<(Vec<u8>, Vec<u8>)>;

// Table definitions
// Key: peer_id bytes, Value: serialized PeerRecord
pub const PEER_REGISTRY: TableDefinition<&[u8], &[u8]> = TableDefinition::new("peer_registry");

// Key: interface_id bytes, Value: serialized InterfaceRecord
pub const INTERFACES: TableDefinition<&[u8], &[u8]> = TableDefinition::new("interfaces");

// Key: (interface_id, peer_id) concatenated, Value: membership info
pub const INTERFACE_MEMBERS: TableDefinition<&[u8], &[u8]> =
    TableDefinition::new("interface_members");

// Key: (peer_id, interface_id) concatenated, Value: serialized SyncStateRecord
pub const SYNC_STATE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("sync_state");

// Key: (interface_id, event_id) concatenated, Value: log offset (u64 as bytes)
pub const EVENT_INDEX: TableDefinition<&[u8], &[u8]> = TableDefinition::new("event_index");

// Key: (peer_id, interface_id, event_id) concatenated, Value: pending delivery metadata
pub const PENDING_DELIVERY: TableDefinition<&[u8], &[u8]> =
    TableDefinition::new("pending_delivery");

// Key: interface_id, Value: serialized SnapshotMetadata
pub const SNAPSHOTS: TableDefinition<&[u8], &[u8]> = TableDefinition::new("snapshots");

/// Configuration for redb storage
#[derive(Debug, Clone)]
pub struct RedbStorageConfig {
    /// Path to the database file
    pub db_path: PathBuf,
    /// Cache size in bytes
    pub cache_size: usize,
}

impl Default for RedbStorageConfig {
    fn default() -> Self {
        Self {
            db_path: PathBuf::from("./data/indras.redb"),
            cache_size: 64 * 1024 * 1024, // 64MB
        }
    }
}

/// Main redb storage manager
pub struct RedbStorage {
    db: Arc<Database>,
    config: RedbStorageConfig,
}

impl RedbStorage {
    /// Open or create the database
    #[instrument(skip(config), fields(path = %config.db_path.display()))]
    pub fn open(config: RedbStorageConfig) -> Result<Self, StorageError> {
        // Ensure parent directory exists
        if let Some(parent) = config.db_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| StorageError::Io(e.to_string()))?;
        }

        let db = Database::create(&config.db_path).map_err(|e| StorageError::Io(e.to_string()))?;

        info!("Opened redb database");

        let storage = Self {
            db: Arc::new(db),
            config,
        };

        // Initialize tables
        storage.init_tables()?;

        Ok(storage)
    }

    /// Initialize all tables
    fn init_tables(&self) -> Result<(), StorageError> {
        let write_txn = self
            .db
            .begin_write()
            .map_err(|e| StorageError::Io(e.to_string()))?;

        // Create tables if they don't exist
        write_txn
            .open_table(PEER_REGISTRY)
            .map_err(|e| StorageError::Io(e.to_string()))?;
        write_txn
            .open_table(INTERFACES)
            .map_err(|e| StorageError::Io(e.to_string()))?;
        write_txn
            .open_table(INTERFACE_MEMBERS)
            .map_err(|e| StorageError::Io(e.to_string()))?;
        write_txn
            .open_table(SYNC_STATE)
            .map_err(|e| StorageError::Io(e.to_string()))?;
        write_txn
            .open_table(EVENT_INDEX)
            .map_err(|e| StorageError::Io(e.to_string()))?;
        write_txn
            .open_table(PENDING_DELIVERY)
            .map_err(|e| StorageError::Io(e.to_string()))?;
        write_txn
            .open_table(SNAPSHOTS)
            .map_err(|e| StorageError::Io(e.to_string()))?;

        write_txn
            .commit()
            .map_err(|e| StorageError::Io(e.to_string()))?;

        debug!("Initialized redb tables");
        Ok(())
    }

    /// Get a reference to the database
    pub fn db(&self) -> &Database {
        &self.db
    }

    /// Get the configuration
    pub fn config(&self) -> &RedbStorageConfig {
        &self.config
    }

    /// Put a key-value pair in a table
    pub fn put(
        &self,
        table: TableDefinition<&[u8], &[u8]>,
        key: &[u8],
        value: &[u8],
    ) -> Result<(), StorageError> {
        let write_txn = self
            .db
            .begin_write()
            .map_err(|e| StorageError::Io(e.to_string()))?;

        {
            let mut table = write_txn
                .open_table(table)
                .map_err(|e| StorageError::Io(e.to_string()))?;
            table
                .insert(key, value)
                .map_err(|e| StorageError::Io(e.to_string()))?;
        }

        write_txn
            .commit()
            .map_err(|e| StorageError::Io(e.to_string()))?;

        Ok(())
    }

    /// Get a value from a table
    pub fn get(
        &self,
        table: TableDefinition<&[u8], &[u8]>,
        key: &[u8],
    ) -> Result<Option<Vec<u8>>, StorageError> {
        let read_txn = self
            .db
            .begin_read()
            .map_err(|e| StorageError::Io(e.to_string()))?;

        let table = read_txn
            .open_table(table)
            .map_err(|e| StorageError::Io(e.to_string()))?;

        let value = table
            .get(key)
            .map_err(|e| StorageError::Io(e.to_string()))?
            .map(|v| v.value().to_vec());

        Ok(value)
    }

    /// Delete a key from a table
    pub fn delete(
        &self,
        table: TableDefinition<&[u8], &[u8]>,
        key: &[u8],
    ) -> Result<bool, StorageError> {
        let write_txn = self
            .db
            .begin_write()
            .map_err(|e| StorageError::Io(e.to_string()))?;

        let removed = {
            let mut table = write_txn
                .open_table(table)
                .map_err(|e| StorageError::Io(e.to_string()))?;
            table
                .remove(key)
                .map_err(|e| StorageError::Io(e.to_string()))?
                .is_some()
        };

        write_txn
            .commit()
            .map_err(|e| StorageError::Io(e.to_string()))?;

        Ok(removed)
    }

    /// Iterate over all entries in a table with a prefix
    pub fn scan_prefix(
        &self,
        table: TableDefinition<&[u8], &[u8]>,
        prefix: &[u8],
    ) -> Result<ScanResults, StorageError> {
        let read_txn = self
            .db
            .begin_read()
            .map_err(|e| StorageError::Io(e.to_string()))?;

        let table = read_txn
            .open_table(table)
            .map_err(|e| StorageError::Io(e.to_string()))?;

        let mut results = Vec::new();

        // Use range to get all keys >= prefix
        let range = table
            .range(prefix..)
            .map_err(|e| StorageError::Io(e.to_string()))?;

        for entry in range {
            let (key, value) = entry.map_err(|e| StorageError::Io(e.to_string()))?;
            let key_bytes = key.value();

            // Stop when we're past the prefix
            if !key_bytes.starts_with(prefix) {
                break;
            }

            results.push((key_bytes.to_vec(), value.value().to_vec()));
        }

        Ok(results)
    }

    /// Count entries with a prefix
    pub fn count_prefix(
        &self,
        table: TableDefinition<&[u8], &[u8]>,
        prefix: &[u8],
    ) -> Result<usize, StorageError> {
        self.scan_prefix(table, prefix).map(|v| v.len())
    }

    /// Compact the database
    ///
    /// Note: redb's compact() requires exclusive access. This may not be
    /// possible with our Arc<Database> design. Consider running this
    /// during maintenance windows.
    pub fn compact(&self) -> Result<(), StorageError> {
        // redb 2.x compact() requires &mut self, which isn't possible with Arc
        // For now, we skip compaction. In production, close and reopen the DB
        // or use a different approach.
        debug!("Database compaction requested (skipped with Arc wrapper)");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_storage() -> (RedbStorage, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let config = RedbStorageConfig {
            db_path: temp_dir.path().join("test.redb"),
            ..Default::default()
        };
        let storage = RedbStorage::open(config).unwrap();
        (storage, temp_dir)
    }

    #[test]
    fn test_put_get() {
        let (storage, _temp) = create_test_storage();

        let key = b"test_key";
        let value = b"test_value";

        storage.put(PEER_REGISTRY, key, value).unwrap();

        let retrieved = storage.get(PEER_REGISTRY, key).unwrap();
        assert_eq!(retrieved, Some(value.to_vec()));
    }

    #[test]
    fn test_delete() {
        let (storage, _temp) = create_test_storage();

        let key = b"delete_me";
        let value = b"value";

        storage.put(PEER_REGISTRY, key, value).unwrap();
        assert!(storage.get(PEER_REGISTRY, key).unwrap().is_some());

        let deleted = storage.delete(PEER_REGISTRY, key).unwrap();
        assert!(deleted);

        assert!(storage.get(PEER_REGISTRY, key).unwrap().is_none());
    }

    #[test]
    fn test_scan_prefix() {
        let (storage, _temp) = create_test_storage();

        // Insert entries with different prefixes
        storage.put(PEER_REGISTRY, b"user:alice", b"data1").unwrap();
        storage.put(PEER_REGISTRY, b"user:bob", b"data2").unwrap();
        storage
            .put(PEER_REGISTRY, b"user:charlie", b"data3")
            .unwrap();
        storage
            .put(PEER_REGISTRY, b"group:admins", b"data4")
            .unwrap();

        // Scan for user prefix
        let users = storage.scan_prefix(PEER_REGISTRY, b"user:").unwrap();
        assert_eq!(users.len(), 3);

        // Scan for group prefix
        let groups = storage.scan_prefix(PEER_REGISTRY, b"group:").unwrap();
        assert_eq!(groups.len(), 1);
    }
}
