//! Persistent storage for encrypted event blobs
//!
//! Stores `InterfaceEventMessage` data as opaque encrypted bytes,
//! indexed by `(interface_id, event_id)` for efficient retrieval.
//! Uses redb as the storage backend with separate tables per storage tier.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use redb::{Database, ReadableTable, ReadableTableMetadata, TableDefinition};
use tracing::{debug, info};

use indras_core::{EventId, InterfaceId};
use indras_transport::protocol::{StorageTier, StoredEvent};

use crate::error::{RelayError, RelayResult};

/// Table: interface_id bytes (32) ++ event sender_hash (8) ++ event sequence (8) → serialized StoredEvent (Self tier)
const SELF_EVENTS_TABLE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("self_events");

/// Table: interface_id bytes (32) → total bytes stored for that interface (Self tier)
const SELF_USAGE_TABLE: TableDefinition<&[u8], u64> = TableDefinition::new("self_usage");

/// Table: interface_id bytes (32) ++ event sender_hash (8) ++ event sequence (8) → serialized StoredEvent (Connections tier)
const CONN_EVENTS_TABLE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("conn_events");

/// Table: interface_id bytes (32) → total bytes stored for that interface (Connections tier)
const CONN_USAGE_TABLE: TableDefinition<&[u8], u64> = TableDefinition::new("conn_usage");

/// Table: interface_id bytes (32) ++ event sender_hash (8) ++ event sequence (8) → serialized StoredEvent (Public tier)
const PUBLIC_EVENTS_TABLE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("pub_events");

/// Table: interface_id bytes (32) → total bytes stored for that interface (Public tier)
const PUBLIC_USAGE_TABLE: TableDefinition<&[u8], u64> = TableDefinition::new("pub_usage");

/// Table: event key (48 bytes) → pin flag (u8, 1 = pinned); pinned events survive cleanup
const PINNED_EVENTS_TABLE: TableDefinition<&[u8], u8> = TableDefinition::new("pinned_events");

/// Table: event key (48 bytes) → TTL override in days (u64)
const TTL_OVERRIDES_TABLE: TableDefinition<&[u8], u64> = TableDefinition::new("ttl_overrides");

/// All storage tiers for iteration
const ALL_TIERS: [StorageTier; 3] = [StorageTier::Self_, StorageTier::Connections, StorageTier::Public];

/// Map a storage tier to its events table definition
fn events_table_for(tier: StorageTier) -> TableDefinition<'static, &'static [u8], &'static [u8]> {
    match tier {
        StorageTier::Self_ => SELF_EVENTS_TABLE,
        StorageTier::Connections => CONN_EVENTS_TABLE,
        StorageTier::Public => PUBLIC_EVENTS_TABLE,
    }
}

/// Map a storage tier to its usage table definition
fn usage_table_for(tier: StorageTier) -> TableDefinition<'static, &'static [u8], u64> {
    match tier {
        StorageTier::Self_ => SELF_USAGE_TABLE,
        StorageTier::Connections => CONN_USAGE_TABLE,
        StorageTier::Public => PUBLIC_USAGE_TABLE,
    }
}

/// Persistent storage for encrypted event blobs
pub struct BlobStore {
    db: Arc<Database>,
}

impl BlobStore {
    /// Create or open a blob store at the given path
    pub fn open(path: &Path) -> RelayResult<Self> {
        std::fs::create_dir_all(path.parent().unwrap_or(path)).map_err(|e| {
            RelayError::Storage(format!("Failed to create database directory: {e}"))
        })?;

        let db = Database::create(path).map_err(|e| {
            RelayError::Storage(format!("Failed to open database: {e}"))
        })?;

        // Initialize all 8 tables (3 event + 3 usage + pinned + ttl_overrides)
        let write_txn = db.begin_write().map_err(|e| {
            RelayError::Storage(format!("Failed to begin write transaction: {e}"))
        })?;
        {
            let _ = write_txn.open_table(SELF_EVENTS_TABLE).map_err(|e| {
                RelayError::Storage(format!("Failed to create self_events table: {e}"))
            })?;
            let _ = write_txn.open_table(SELF_USAGE_TABLE).map_err(|e| {
                RelayError::Storage(format!("Failed to create self_usage table: {e}"))
            })?;
            let _ = write_txn.open_table(CONN_EVENTS_TABLE).map_err(|e| {
                RelayError::Storage(format!("Failed to create conn_events table: {e}"))
            })?;
            let _ = write_txn.open_table(CONN_USAGE_TABLE).map_err(|e| {
                RelayError::Storage(format!("Failed to create conn_usage table: {e}"))
            })?;
            let _ = write_txn.open_table(PUBLIC_EVENTS_TABLE).map_err(|e| {
                RelayError::Storage(format!("Failed to create pub_events table: {e}"))
            })?;
            let _ = write_txn.open_table(PUBLIC_USAGE_TABLE).map_err(|e| {
                RelayError::Storage(format!("Failed to create pub_usage table: {e}"))
            })?;
            let _ = write_txn.open_table(PINNED_EVENTS_TABLE).map_err(|e| {
                RelayError::Storage(format!("Failed to create pinned_events table: {e}"))
            })?;
            let _ = write_txn.open_table(TTL_OVERRIDES_TABLE).map_err(|e| {
                RelayError::Storage(format!("Failed to create ttl_overrides table: {e}"))
            })?;
        }
        write_txn.commit().map_err(|e| {
            RelayError::Storage(format!("Failed to commit initial tables: {e}"))
        })?;

        Ok(Self { db: Arc::new(db) })
    }

    /// Store an encrypted event blob (defaults to Connections tier)
    pub fn store_event(
        &self,
        interface_id: InterfaceId,
        event: &StoredEvent,
    ) -> RelayResult<()> {
        self.store_event_tiered(StorageTier::Connections, interface_id, event)
    }

    /// Store an encrypted event blob in a specific tier
    pub fn store_event_tiered(
        &self,
        tier: StorageTier,
        interface_id: InterfaceId,
        event: &StoredEvent,
    ) -> RelayResult<()> {
        let key = make_event_key(&interface_id, &event.event_id);
        let value = postcard::to_allocvec(event).map_err(|e| {
            RelayError::Serialization(format!("Failed to serialize event: {e}"))
        })?;

        let event_size = value.len() as u64;

        let write_txn = self.db.begin_write().map_err(|e| {
            RelayError::Storage(format!("Failed to begin write: {e}"))
        })?;
        {
            let mut table = write_txn.open_table(events_table_for(tier)).map_err(|e| {
                RelayError::Storage(format!("Failed to open events table: {e}"))
            })?;
            table.insert(key.as_slice(), value.as_slice()).map_err(|e| {
                RelayError::Storage(format!("Failed to insert event: {e}"))
            })?;

            // Update usage
            let mut usage_table = write_txn.open_table(usage_table_for(tier)).map_err(|e| {
                RelayError::Storage(format!("Failed to open usage table: {e}"))
            })?;
            let iface_key = interface_id.0.as_slice();
            let current = usage_table
                .get(iface_key)
                .map_err(|e| RelayError::Storage(format!("Failed to get usage: {e}")))?
                .map(|v| v.value())
                .unwrap_or(0);
            usage_table.insert(iface_key, current + event_size).map_err(|e| {
                RelayError::Storage(format!("Failed to update usage: {e}"))
            })?;
        }
        write_txn.commit().map_err(|e| {
            RelayError::Storage(format!("Failed to commit event: {e}"))
        })?;

        debug!(
            interface = ?hex::short(&interface_id.0),
            event_id = ?event.event_id,
            ?tier,
            "Stored event"
        );
        Ok(())
    }

    /// Retrieve events after a given event_id for an interface (defaults to Connections tier)
    ///
    /// If `after` is None, returns all events for the interface.
    pub fn events_after(
        &self,
        interface_id: InterfaceId,
        after: Option<EventId>,
    ) -> RelayResult<Vec<StoredEvent>> {
        self.events_after_tiered(StorageTier::Connections, interface_id, after)
    }

    /// Retrieve events after a given event_id for an interface in a specific tier
    ///
    /// If `after` is None, returns all events for the interface.
    pub fn events_after_tiered(
        &self,
        tier: StorageTier,
        interface_id: InterfaceId,
        after: Option<EventId>,
    ) -> RelayResult<Vec<StoredEvent>> {
        let read_txn = self.db.begin_read().map_err(|e| {
            RelayError::Storage(format!("Failed to begin read: {e}"))
        })?;
        let table = read_txn.open_table(events_table_for(tier)).map_err(|e| {
            RelayError::Storage(format!("Failed to open events table: {e}"))
        })?;

        let prefix = interface_id.0.to_vec();
        let mut results = Vec::new();

        // Scan all entries with this interface prefix
        let range = table.range(prefix.as_slice()..).map_err(|e| {
            RelayError::Storage(format!("Failed to range scan: {e}"))
        })?;

        for entry in range {
            let (key, value) = entry.map_err(|e| {
                RelayError::Storage(format!("Failed to read entry: {e}"))
            })?;

            let key_bytes = key.value();
            // Check this key belongs to our interface (first 32 bytes)
            if key_bytes.len() < 32 || &key_bytes[..32] != interface_id.0.as_slice() {
                break; // Past our interface prefix
            }

            let event: StoredEvent = postcard::from_bytes(value.value()).map_err(|e| {
                RelayError::Serialization(format!("Failed to deserialize event: {e}"))
            })?;

            // Filter by after_event_id if provided
            if let Some(ref after_id) = after {
                if event.event_id.sender_hash < after_id.sender_hash
                    || (event.event_id.sender_hash == after_id.sender_hash
                        && event.event_id.sequence <= after_id.sequence)
                {
                    continue;
                }
            }

            results.push(event);
        }

        Ok(results)
    }

    /// Get total storage usage for an interface in bytes (across all tiers)
    pub fn interface_usage_bytes(&self, interface_id: &InterfaceId) -> RelayResult<u64> {
        let mut total = 0u64;
        for tier in ALL_TIERS {
            let read_txn = self.db.begin_read().map_err(|e| {
                RelayError::Storage(format!("Failed to begin read: {e}"))
            })?;
            let table = read_txn.open_table(usage_table_for(tier)).map_err(|e| {
                RelayError::Storage(format!("Failed to open usage table: {e}"))
            })?;
            total += table
                .get(interface_id.0.as_slice())
                .map_err(|e| RelayError::Storage(format!("Failed to get usage: {e}")))?
                .map(|v| v.value())
                .unwrap_or(0);
        }
        Ok(total)
    }

    /// Get total storage usage across all interfaces and all tiers
    pub fn total_usage_bytes(&self) -> RelayResult<u64> {
        let mut total = 0u64;
        for tier in ALL_TIERS {
            total += self.tier_usage_bytes(tier)?;
        }
        Ok(total)
    }

    /// Get total storage usage for a specific tier across all interfaces
    pub fn tier_usage_bytes(&self, tier: StorageTier) -> RelayResult<u64> {
        let read_txn = self.db.begin_read().map_err(|e| {
            RelayError::Storage(format!("Failed to begin read: {e}"))
        })?;
        let table = read_txn.open_table(usage_table_for(tier)).map_err(|e| {
            RelayError::Storage(format!("Failed to open usage table: {e}"))
        })?;

        let mut total = 0u64;
        let iter = table.iter().map_err(|e| {
            RelayError::Storage(format!("Failed to iterate usage: {e}"))
        })?;
        for entry in iter {
            let (_, value) = entry.map_err(|e| {
                RelayError::Storage(format!("Failed to read usage entry: {e}"))
            })?;
            total += value.value();
        }

        Ok(total)
    }

    /// Evict all events for an interface across all tiers
    pub fn evict_interface(&self, interface_id: &InterfaceId) -> RelayResult<usize> {
        let prefix = interface_id.0.to_vec();
        let mut count = 0;

        let write_txn = self.db.begin_write().map_err(|e| {
            RelayError::Storage(format!("Failed to begin write: {e}"))
        })?;

        for tier in ALL_TIERS {
            let mut table = write_txn.open_table(events_table_for(tier)).map_err(|e| {
                RelayError::Storage(format!("Failed to open events table: {e}"))
            })?;

            // Collect keys to delete
            let mut keys_to_delete = Vec::new();
            {
                let range = table.range(prefix.as_slice()..).map_err(|e| {
                    RelayError::Storage(format!("Failed to range scan: {e}"))
                })?;
                for entry in range {
                    let (key, _) = entry.map_err(|e| {
                        RelayError::Storage(format!("Failed to read entry: {e}"))
                    })?;
                    let key_bytes = key.value();
                    if key_bytes.len() < 32 || &key_bytes[..32] != interface_id.0.as_slice() {
                        break;
                    }
                    keys_to_delete.push(key_bytes.to_vec());
                }
            }

            for key in &keys_to_delete {
                table.remove(key.as_slice()).map_err(|e| {
                    RelayError::Storage(format!("Failed to remove event: {e}"))
                })?;
                count += 1;
            }

            // Reset usage for this tier
            let mut usage_table = write_txn.open_table(usage_table_for(tier)).map_err(|e| {
                RelayError::Storage(format!("Failed to open usage table: {e}"))
            })?;
            usage_table.remove(interface_id.0.as_slice()).map_err(|e| {
                RelayError::Storage(format!("Failed to remove usage: {e}"))
            })?;
        }

        write_txn.commit().map_err(|e| {
            RelayError::Storage(format!("Failed to commit eviction: {e}"))
        })?;

        info!(interface = ?hex::short(&interface_id.0), count, "Evicted interface events");
        Ok(count)
    }

    /// Mark an event as pinned so it survives cleanup
    pub fn pin_event(
        &self,
        _tier: StorageTier,
        interface_id: &InterfaceId,
        event_id: &EventId,
    ) -> RelayResult<()> {
        let key = make_event_key(interface_id, event_id);
        let write_txn = self.db.begin_write().map_err(|e| {
            RelayError::Storage(format!("Failed to begin write: {e}"))
        })?;
        {
            let mut table = write_txn.open_table(PINNED_EVENTS_TABLE).map_err(|e| {
                RelayError::Storage(format!("Failed to open pinned table: {e}"))
            })?;
            table.insert(key.as_slice(), 1u8).map_err(|e| {
                RelayError::Storage(format!("Failed to pin event: {e}"))
            })?;
        }
        write_txn.commit().map_err(|e| {
            RelayError::Storage(format!("Failed to commit pin: {e}"))
        })?;
        Ok(())
    }

    /// Set a TTL override for an event (overrides the tier default during cleanup)
    pub fn set_ttl_override(
        &self,
        interface_id: &InterfaceId,
        event_id: &EventId,
        ttl_days: u64,
    ) -> RelayResult<()> {
        let key = make_event_key(interface_id, event_id);
        let write_txn = self.db.begin_write().map_err(|e| {
            RelayError::Storage(format!("Failed to begin write: {e}"))
        })?;
        {
            let mut table = write_txn.open_table(TTL_OVERRIDES_TABLE).map_err(|e| {
                RelayError::Storage(format!("Failed to open TTL overrides table: {e}"))
            })?;
            table.insert(key.as_slice(), ttl_days).map_err(|e| {
                RelayError::Storage(format!("Failed to set TTL override: {e}"))
            })?;
        }
        write_txn.commit().map_err(|e| {
            RelayError::Storage(format!("Failed to commit TTL override: {e}"))
        })?;
        Ok(())
    }

    /// Check whether an event is pinned
    pub fn is_pinned(&self, interface_id: &InterfaceId, event_id: &EventId) -> RelayResult<bool> {
        let key = make_event_key(interface_id, event_id);
        let read_txn = self.db.begin_read().map_err(|e| {
            RelayError::Storage(format!("Failed to begin read: {e}"))
        })?;
        let table = read_txn.open_table(PINNED_EVENTS_TABLE).map_err(|e| {
            RelayError::Storage(format!("Failed to open pinned table: {e}"))
        })?;
        Ok(table
            .get(key.as_slice())
            .map_err(|e| RelayError::Storage(format!("Failed to check pin: {e}")))?
            .is_some())
    }

    /// Get the TTL override for an event, if any
    pub fn get_ttl_override(
        &self,
        interface_id: &InterfaceId,
        event_id: &EventId,
    ) -> RelayResult<Option<u64>> {
        let key = make_event_key(interface_id, event_id);
        let read_txn = self.db.begin_read().map_err(|e| {
            RelayError::Storage(format!("Failed to begin read: {e}"))
        })?;
        let table = read_txn.open_table(TTL_OVERRIDES_TABLE).map_err(|e| {
            RelayError::Storage(format!("Failed to open TTL overrides table: {e}"))
        })?;
        Ok(table
            .get(key.as_slice())
            .map_err(|e| RelayError::Storage(format!("Failed to get TTL override: {e}")))?
            .map(|v| v.value()))
    }

    /// Clean up events older than the given duration across all tiers
    pub fn cleanup_expired(&self, max_age: Duration) -> RelayResult<usize> {
        let mut total = 0;
        for tier in ALL_TIERS {
            total += self.cleanup_expired_tiered(tier, max_age)?;
        }
        Ok(total)
    }

    /// Clean up expired events in a specific tier
    ///
    /// Pinned events are never removed. Events with a TTL override use that override
    /// instead of the tier default `max_age`.
    pub fn cleanup_expired_tiered(&self, tier: StorageTier, max_age: Duration) -> RelayResult<usize> {
        let now_millis = chrono::Utc::now().timestamp_millis();
        let cutoff = now_millis - max_age.as_millis() as i64;
        let mut count = 0;

        let write_txn = self.db.begin_write().map_err(|e| {
            RelayError::Storage(format!("Failed to begin write: {e}"))
        })?;
        {
            let mut events_table = write_txn.open_table(events_table_for(tier)).map_err(|e| {
                RelayError::Storage(format!("Failed to open events table: {e}"))
            })?;
            let pinned_table = write_txn.open_table(PINNED_EVENTS_TABLE).map_err(|e| {
                RelayError::Storage(format!("Failed to open pinned table: {e}"))
            })?;
            let ttl_table = write_txn.open_table(TTL_OVERRIDES_TABLE).map_err(|e| {
                RelayError::Storage(format!("Failed to open TTL table: {e}"))
            })?;

            // Collect expired keys
            let mut keys_to_delete = Vec::new();
            {
                let iter = events_table.iter().map_err(|e| {
                    RelayError::Storage(format!("Failed to iterate events: {e}"))
                })?;
                for entry in iter {
                    let (key, value) = entry.map_err(|e| {
                        RelayError::Storage(format!("Failed to read entry: {e}"))
                    })?;
                    let event: StoredEvent = match postcard::from_bytes(value.value()) {
                        Ok(e) => e,
                        Err(_) => continue,
                    };

                    let key_bytes = key.value().to_vec();

                    // Skip pinned events
                    if pinned_table
                        .get(key_bytes.as_slice())
                        .map_err(|e| RelayError::Storage(format!("Failed to check pin: {e}")))?
                        .is_some()
                    {
                        continue;
                    }

                    // Use TTL override if present, otherwise the tier default cutoff
                    let effective_cutoff = if let Some(override_val) = ttl_table
                        .get(key_bytes.as_slice())
                        .map_err(|e| RelayError::Storage(format!("Failed to get TTL override: {e}")))?
                    {
                        now_millis - (override_val.value() as i64 * 86_400_000)
                    } else {
                        cutoff
                    };

                    if event.received_at_millis < effective_cutoff {
                        keys_to_delete.push(key_bytes);
                    }
                }
            }

            for key in &keys_to_delete {
                events_table.remove(key.as_slice()).map_err(|e| {
                    RelayError::Storage(format!("Failed to remove expired: {e}"))
                })?;
                count += 1;
            }
        }
        write_txn.commit().map_err(|e| {
            RelayError::Storage(format!("Failed to commit cleanup: {e}"))
        })?;

        if count > 0 {
            info!(count, ?tier, "Cleaned up expired events");
        }
        Ok(count)
    }

    /// Count total events stored across all tiers
    pub fn event_count(&self) -> RelayResult<usize> {
        let mut total = 0usize;
        for tier in ALL_TIERS {
            let read_txn = self.db.begin_read().map_err(|e| {
                RelayError::Storage(format!("Failed to begin read: {e}"))
            })?;
            let table = read_txn.open_table(events_table_for(tier)).map_err(|e| {
                RelayError::Storage(format!("Failed to open events table: {e}"))
            })?;
            total += table.len().map_err(|e| {
                RelayError::Storage(format!("Failed to count events: {e}"))
            })? as usize;
        }
        Ok(total)
    }

    /// Count events for a specific interface across all tiers
    pub fn interface_event_count(&self, interface_id: &InterfaceId) -> RelayResult<usize> {
        let mut total = 0usize;
        for tier in ALL_TIERS {
            let read_txn = self.db.begin_read().map_err(|e| {
                RelayError::Storage(format!("Failed to begin read: {e}"))
            })?;
            let table = read_txn.open_table(events_table_for(tier)).map_err(|e| {
                RelayError::Storage(format!("Failed to open events table: {e}"))
            })?;

            let prefix = interface_id.0.to_vec();
            let range = table.range(prefix.as_slice()..).map_err(|e| {
                RelayError::Storage(format!("Failed to range scan: {e}"))
            })?;
            for entry in range {
                let (key, _) = entry.map_err(|e| {
                    RelayError::Storage(format!("Failed to read entry: {e}"))
                })?;
                if key.value().len() < 32 || &key.value()[..32] != interface_id.0.as_slice() {
                    break;
                }
                total += 1;
            }
        }
        Ok(total)
    }
}

/// Create a composite key: interface_id (32) ++ sender_hash (8) ++ sequence (8)
fn make_event_key(interface_id: &InterfaceId, event_id: &EventId) -> Vec<u8> {
    let mut key = Vec::with_capacity(48);
    key.extend_from_slice(&interface_id.0);
    key.extend_from_slice(&event_id.sender_hash.to_be_bytes());
    key.extend_from_slice(&event_id.sequence.to_be_bytes());
    key
}

/// Helper for short hex display
mod hex {
    pub fn short(bytes: &[u8]) -> String {
        if bytes.len() >= 4 {
            format!(
                "{:02x}{:02x}..{:02x}{:02x}",
                bytes[0],
                bytes[1],
                bytes[bytes.len() - 2],
                bytes[bytes.len() - 1]
            )
        } else {
            bytes.iter().map(|b| format!("{b:02x}")).collect()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_store() -> (BlobStore, TempDir) {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("test.redb");
        let store = BlobStore::open(&db_path).unwrap();
        (store, dir)
    }

    #[test]
    fn test_store_and_retrieve() {
        let (store, _dir) = test_store();
        let iface = InterfaceId::new([0x42; 32]);

        let event1 = StoredEvent::new(EventId::new(1, 1), vec![10, 20, 30], [0x11; 12]);
        let event2 = StoredEvent::new(EventId::new(1, 2), vec![40, 50, 60], [0x22; 12]);

        store.store_event(iface, &event1).unwrap();
        store.store_event(iface, &event2).unwrap();

        let events = store.events_after(iface, None).unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].encrypted_event, vec![10, 20, 30]);
        assert_eq!(events[1].encrypted_event, vec![40, 50, 60]);
    }

    #[test]
    fn test_events_after_filter() {
        let (store, _dir) = test_store();
        let iface = InterfaceId::new([0x42; 32]);

        for i in 1..=5 {
            let event = StoredEvent::new(EventId::new(1, i), vec![i as u8], [0x11; 12]);
            store.store_event(iface, &event).unwrap();
        }

        // Get events after sequence 3
        let events = store
            .events_after(iface, Some(EventId::new(1, 3)))
            .unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].event_id.sequence, 4);
        assert_eq!(events[1].event_id.sequence, 5);
    }

    #[test]
    fn test_evict_interface() {
        let (store, _dir) = test_store();
        let iface1 = InterfaceId::new([0x11; 32]);
        let iface2 = InterfaceId::new([0x22; 32]);

        store
            .store_event(iface1, &StoredEvent::new(EventId::new(1, 1), vec![1], [0; 12]))
            .unwrap();
        store
            .store_event(iface2, &StoredEvent::new(EventId::new(1, 1), vec![2], [0; 12]))
            .unwrap();

        let evicted = store.evict_interface(&iface1).unwrap();
        assert_eq!(evicted, 1);

        assert_eq!(store.events_after(iface1, None).unwrap().len(), 0);
        assert_eq!(store.events_after(iface2, None).unwrap().len(), 1);
    }

    #[test]
    fn test_usage_tracking() {
        let (store, _dir) = test_store();
        let iface = InterfaceId::new([0x42; 32]);

        assert_eq!(store.interface_usage_bytes(&iface).unwrap(), 0);

        store
            .store_event(
                iface,
                &StoredEvent::new(EventId::new(1, 1), vec![0; 100], [0; 12]),
            )
            .unwrap();

        assert!(store.interface_usage_bytes(&iface).unwrap() > 0);
        assert!(store.total_usage_bytes().unwrap() > 0);
    }

    #[test]
    fn test_event_count() {
        let (store, _dir) = test_store();
        let iface = InterfaceId::new([0x42; 32]);

        assert_eq!(store.event_count().unwrap(), 0);

        for i in 1..=3 {
            store
                .store_event(iface, &StoredEvent::new(EventId::new(1, i), vec![i as u8], [0; 12]))
                .unwrap();
        }

        assert_eq!(store.event_count().unwrap(), 3);
        assert_eq!(store.interface_event_count(&iface).unwrap(), 3);
    }

    #[test]
    fn test_cleanup_expired() {
        let (store, _dir) = test_store();
        let iface = InterfaceId::new([0x42; 32]);

        // Store an event with an old timestamp
        let mut old_event = StoredEvent::new(EventId::new(1, 1), vec![1], [0; 12]);
        old_event.received_at_millis = 1000; // Very old

        store.store_event(iface, &old_event).unwrap();

        // Store a recent event
        let recent_event = StoredEvent::new(EventId::new(1, 2), vec![2], [0; 12]);
        store.store_event(iface, &recent_event).unwrap();

        // Cleanup events older than 1 day
        let cleaned = store.cleanup_expired(Duration::from_secs(86400)).unwrap();
        assert_eq!(cleaned, 1);
        assert_eq!(store.event_count().unwrap(), 1);
    }

    #[test]
    fn test_tiered_store_and_retrieve() {
        let (store, _dir) = test_store();
        let iface = InterfaceId::new([0x42; 32]);

        let event1 = StoredEvent::new(EventId::new(1, 1), vec![10, 20, 30], [0x11; 12]);
        let event2 = StoredEvent::new(EventId::new(1, 2), vec![40, 50, 60], [0x22; 12]);

        store.store_event_tiered(StorageTier::Self_, iface, &event1).unwrap();
        store.store_event_tiered(StorageTier::Public, iface, &event2).unwrap();

        // Each tier has its own events
        let self_events = store.events_after_tiered(StorageTier::Self_, iface, None).unwrap();
        assert_eq!(self_events.len(), 1);
        assert_eq!(self_events[0].encrypted_event, vec![10, 20, 30]);

        let public_events = store.events_after_tiered(StorageTier::Public, iface, None).unwrap();
        assert_eq!(public_events.len(), 1);
        assert_eq!(public_events[0].encrypted_event, vec![40, 50, 60]);

        // Connections tier is empty
        let conn_events = store.events_after_tiered(StorageTier::Connections, iface, None).unwrap();
        assert_eq!(conn_events.len(), 0);
    }

    #[test]
    fn test_tier_usage_bytes() {
        let (store, _dir) = test_store();
        let iface = InterfaceId::new([0x42; 32]);

        store.store_event_tiered(StorageTier::Self_, iface,
            &StoredEvent::new(EventId::new(1, 1), vec![0; 100], [0; 12])).unwrap();
        store.store_event_tiered(StorageTier::Public, iface,
            &StoredEvent::new(EventId::new(1, 2), vec![0; 50], [0; 12])).unwrap();

        assert!(store.tier_usage_bytes(StorageTier::Self_).unwrap() > 0);
        assert!(store.tier_usage_bytes(StorageTier::Public).unwrap() > 0);
        assert_eq!(store.tier_usage_bytes(StorageTier::Connections).unwrap(), 0);
    }

    #[test]
    fn test_pinned_events_survive_cleanup() {
        let (store, _dir) = test_store();
        let iface = InterfaceId::new([0x42; 32]);

        let event_id_1 = EventId::new(1, 1);
        let event_id_2 = EventId::new(1, 2);
        let mut event1 = StoredEvent::new(event_id_1, vec![1], [0; 12]);
        event1.received_at_millis = 1000; // very old
        let mut event2 = StoredEvent::new(event_id_2, vec![2], [0; 12]);
        event2.received_at_millis = 1000; // very old

        store.store_event_tiered(StorageTier::Connections, iface, &event1).unwrap();
        store.store_event_tiered(StorageTier::Connections, iface, &event2).unwrap();

        // Pin event1
        store.pin_event(StorageTier::Connections, &iface, &event_id_1).unwrap();
        assert!(store.is_pinned(&iface, &event_id_1).unwrap());
        assert!(!store.is_pinned(&iface, &event_id_2).unwrap());

        // Cleanup should remove event2 but not event1
        let cleaned = store.cleanup_expired_tiered(StorageTier::Connections, Duration::from_secs(86400)).unwrap();
        assert_eq!(cleaned, 1);
        assert_eq!(store.events_after_tiered(StorageTier::Connections, iface, None).unwrap().len(), 1);
    }

    #[test]
    fn test_ttl_override() {
        let (store, _dir) = test_store();
        let iface = InterfaceId::new([0x42; 32]);

        let event_id = EventId::new(1, 1);
        let mut event = StoredEvent::new(event_id, vec![1], [0; 12]);
        // 2 days ago
        event.received_at_millis = chrono::Utc::now().timestamp_millis() - 2 * 86_400_000;

        store.store_event_tiered(StorageTier::Connections, iface, &event).unwrap();

        // Set TTL override to 30 days
        store.set_ttl_override(&iface, &event_id, 30).unwrap();
        assert_eq!(store.get_ttl_override(&iface, &event_id).unwrap(), Some(30));

        // Cleanup with 1-day default TTL should NOT remove the event (override is 30 days)
        let cleaned = store.cleanup_expired_tiered(StorageTier::Connections, Duration::from_secs(86400)).unwrap();
        assert_eq!(cleaned, 0);

        // Event is only 2 days old vs 30-day override — still survives
        let cleaned = store.cleanup_expired_tiered(StorageTier::Connections, Duration::from_secs(60 * 86400)).unwrap();
        assert_eq!(cleaned, 0);
    }
}
