//! Local-only trust store for per-peer merge consent.
//!
//! Trust decisions are never synced via CRDT — they are private to
//! each peer. A trusted peer's changes auto-merge; an untrusted
//! peer's changes appear as forks requiring explicit merge consent.

use crate::vault::vault_file::UserId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Local-only store of per-peer trust decisions.
///
/// Persists to `<vault_path>/.indras/trust.json`. Never enters any
/// CRDT document.
#[derive(Debug, Clone)]
pub struct LocalTrustStore {
    path: PathBuf,
    trust: HashMap<UserId, bool>,
}

/// On-disk format for the trust store.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct TrustFile {
    /// Peer trust entries keyed by hex-encoded UserId.
    peers: HashMap<String, bool>,
}

impl LocalTrustStore {
    /// Load or create a trust store for the given vault directory.
    pub async fn load(vault_path: &Path) -> Self {
        let dir = vault_path.join(".indras");
        let path = dir.join("trust.json");
        let trust = match tokio::fs::read(&path).await {
            Ok(bytes) => {
                let tf: TrustFile =
                    serde_json::from_slice(&bytes).unwrap_or_default();
                tf.peers
                    .into_iter()
                    .filter_map(|(hex, trusted)| {
                        let bytes = hex::decode(&hex).ok()?;
                        let arr: [u8; 32] = bytes.try_into().ok()?;
                        Some((arr, trusted))
                    })
                    .collect()
            }
            Err(_) => HashMap::new(),
        };
        Self { path, trust }
    }

    /// Whether the given peer is trusted (auto-merge enabled).
    ///
    /// Unknown peers default to untrusted.
    pub fn is_trusted(&self, peer_id: &UserId) -> bool {
        self.trust.get(peer_id).copied().unwrap_or(false)
    }

    /// Set the trust level for a peer and persist to disk.
    pub async fn set_trust(&mut self, peer_id: UserId, trusted: bool) {
        self.trust.insert(peer_id, trusted);
        let _ = self.save().await;
    }

    /// Persist the current trust state to disk.
    async fn save(&self) -> std::io::Result<()> {
        if let Some(parent) = self.path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let tf = TrustFile {
            peers: self
                .trust
                .iter()
                .map(|(id, trusted)| (hex::encode(id), *trusted))
                .collect(),
        };
        let bytes = serde_json::to_vec_pretty(&tf)
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        tokio::fs::write(&self.path, bytes).await
    }
}
