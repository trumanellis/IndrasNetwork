//! In-band steward share delivery over iroh DM realms.
//!
//! Sender-side: when a user nominates a peer as a steward, the
//! encrypted share is published as a CRDT document keyed
//! `_steward_share:{sender_user_id_hex}` inside the sender↔steward DM
//! realm. The steward's node ingests it via ambient realm sync and
//! materializes a local holdings cache.
//!
//! Steward-side: [`scan_held_backups`] walks the node's DM realms and,
//! for every non-self peer with a KEM key on file, probes the
//! matching share-delivery doc. Hits are written to
//! `<data_dir>/steward_holdings.json` and surfaced to the UI as a
//! status-bar badge on the Backup-plan link.
//!
//! One share per sender. Re-splits by the same sender (bumped
//! `secret_version`) overwrite the prior entry under the same key —
//! stewards always hold the latest share.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use indras_network::document::DocumentSchema;

/// Filename for the on-disk steward holdings cache.
const HOLDINGS_FILENAME: &str = "steward_holdings.json";

/// Key prefix every share-delivery doc uses inside a DM realm.
pub const SHARE_DELIVERY_KEY_PREFIX: &str = "_steward_share:";

/// Compute the doc key a sender writes into their DM realm with a
/// specific steward. Readers (the steward) reconstruct the same key
/// for each non-self peer's UserId found in the realm's peer-keys
/// directory.
pub fn share_delivery_doc_key(sender_user_id: &[u8; 32]) -> String {
    format!("{}{}", SHARE_DELIVERY_KEY_PREFIX, hex::encode(sender_user_id))
}

/// CRDT document holding one encrypted steward share.
///
/// Last-writer-wins on `created_at_millis` so a re-split cleanly
/// supersedes prior state without requiring deletion.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ShareDelivery {
    /// `EncryptedStewardShare::to_bytes()` payload. Empty in the
    /// default/zero state so readers can distinguish "no delivery yet".
    pub encrypted_share: Vec<u8>,
    /// The sender's `UserId` (blake3 of their PQ verifying key).
    pub sender_user_id: [u8; 32],
    /// Wall-clock millis when the share was sealed and published.
    pub created_at_millis: i64,
    /// Human-readable hint, e.g. "Truman's backup piece".
    pub label: String,
}

impl DocumentSchema for ShareDelivery {
    fn merge(&mut self, remote: Self) {
        if remote.created_at_millis > self.created_at_millis {
            *self = remote;
        }
    }
}

/// One entry in the local holdings cache — a backup the user is
/// keeping on someone else's behalf.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HeldBackup {
    /// The sender's `UserId` hex — the subject who may one day ask
    /// for this piece back.
    pub sender_user_id_hex: String,
    /// Caller-supplied label at publication time. May be empty.
    pub label: String,
    /// Wall-clock millis captured at publication.
    pub created_at_millis: i64,
}

/// Serializable wrapper for the on-disk holdings cache.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StewardHoldings {
    /// One entry per sender whose share we hold. Keyed by sender
    /// UserId hex so re-publishes overwrite in place.
    pub by_sender: std::collections::BTreeMap<String, HeldBackup>,
}

impl StewardHoldings {
    /// Persist the holdings cache under `<data_dir>/steward_holdings.json`.
    pub fn save(&self, data_dir: &Path) -> std::io::Result<()> {
        let bytes = serde_json::to_vec_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(holdings_path(data_dir), bytes)
    }

    /// Load the holdings cache, or return a fresh empty one if the
    /// file does not exist yet.
    pub fn load(data_dir: &Path) -> std::io::Result<Self> {
        let path = holdings_path(data_dir);
        if !path.exists() {
            return Ok(Self::default());
        }
        let bytes = std::fs::read(&path)?;
        serde_json::from_slice(&bytes)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }

    /// Number of distinct senders we hold a share for.
    pub fn count(&self) -> usize {
        self.by_sender.len()
    }
}

fn holdings_path(data_dir: &Path) -> PathBuf {
    data_dir.join(HOLDINGS_FILENAME)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn doc_key_is_stable_for_fixed_uid() {
        let uid = [0xabu8; 32];
        let k1 = share_delivery_doc_key(&uid);
        let k2 = share_delivery_doc_key(&uid);
        assert_eq!(k1, k2);
        assert!(k1.starts_with(SHARE_DELIVERY_KEY_PREFIX));
        assert_eq!(k1.len(), SHARE_DELIVERY_KEY_PREFIX.len() + 64);
    }

    #[test]
    fn share_delivery_merge_prefers_newer() {
        let older = ShareDelivery {
            encrypted_share: vec![1, 2, 3],
            sender_user_id: [1u8; 32],
            created_at_millis: 1_000,
            label: "old".into(),
        };
        let newer = ShareDelivery {
            encrypted_share: vec![9, 9, 9],
            sender_user_id: [1u8; 32],
            created_at_millis: 5_000,
            label: "new".into(),
        };

        let mut a = older.clone();
        a.merge(newer.clone());
        assert_eq!(a.created_at_millis, 5_000);
        assert_eq!(a.label, "new");

        let mut b = newer.clone();
        b.merge(older);
        assert_eq!(b.created_at_millis, 5_000);
        assert_eq!(b.label, "new");
    }

    #[test]
    fn holdings_roundtrip_empty_then_populated() {
        let dir = TempDir::new().unwrap();
        let data = dir.path();

        let first = StewardHoldings::load(data).unwrap();
        assert_eq!(first.count(), 0);

        let mut h = StewardHoldings::default();
        h.by_sender.insert(
            "aa".to_string(),
            HeldBackup {
                sender_user_id_hex: "aa".into(),
                label: "A's piece".into(),
                created_at_millis: 7,
            },
        );
        h.save(data).unwrap();

        let reloaded = StewardHoldings::load(data).unwrap();
        assert_eq!(reloaded.count(), 1);
        assert_eq!(reloaded.by_sender["aa"].label, "A's piece");
    }
}
