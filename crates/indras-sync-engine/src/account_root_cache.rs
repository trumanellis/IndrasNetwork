//! On-disk stash for the account root's signing-key bytes between
//! account creation and the first successful steward split.
//!
//! At creation time the root `sk` must live somewhere the device
//! can re-read it when stewards eventually accept the invitation
//! (possibly hours later, across an app restart). We keep it in a
//! file alongside the existing plaintext PQ identity — no worse
//! than the current Phase-1 security posture — and delete it the
//! moment [`finalize_steward_split`](crate::recovery_protocol)
//! successfully distributes the shares.
//!
//! The file contains the serialized `(sk_bytes, vk_bytes)` pair so
//! the device can rebuild a full `AccountRoot` without re-reading
//! the home-realm roster doc.
//!
//! **Security note.** This is a temporary compromise. Phase 2+
//! will store the `sk` wrapped by hardware-key or passkey attestation
//! so a stolen device can't use it before the split completes.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use indras_crypto::account_root::AccountRoot;

/// Filename for the pending root cache.
const PENDING_ROOT_FILENAME: &str = "account_root.pending";

/// Serializable on-disk shape — separate from `AccountRoot` so we
/// can evolve on-disk layout without coupling the crypto type.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PendingRootRecord {
    sk_bytes: Vec<u8>,
    vk_bytes: Vec<u8>,
}

fn cache_path(data_dir: &Path) -> PathBuf {
    data_dir.join(PENDING_ROOT_FILENAME)
}

/// Persist a freshly-minted root so `finalize_steward_split` can
/// pick it up when quorum lands.
pub fn save_pending_root(data_dir: &Path, root: &AccountRoot) -> std::io::Result<()> {
    let (sk, vk) = root.to_keypair_bytes();
    let record = PendingRootRecord {
        sk_bytes: sk.as_slice().to_vec(),
        vk_bytes: vk,
    };
    let bytes = serde_json::to_vec(&record)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(cache_path(data_dir), bytes)
}

/// Load the pending root. Returns `None` when the cache has been
/// cleared — i.e., the split already succeeded and no re-issue is
/// needed.
pub fn load_pending_root(data_dir: &Path) -> Option<AccountRoot> {
    let bytes = std::fs::read(cache_path(data_dir)).ok()?;
    let record: PendingRootRecord = serde_json::from_slice(&bytes).ok()?;
    AccountRoot::from_keypair_bytes(&record.sk_bytes, &record.vk_bytes).ok()
}

/// Delete the pending root cache. Called after a successful steward
/// split so a stolen device no longer has the sk on disk.
pub fn clear_pending_root(data_dir: &Path) {
    let _ = std::fs::remove_file(cache_path(data_dir));
}

/// Returns `true` when a pending root is present.
pub fn has_pending_root(data_dir: &Path) -> bool {
    cache_path(data_dir).exists()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn roundtrip_through_disk() {
        let dir = TempDir::new().unwrap();
        let root = AccountRoot::generate();
        save_pending_root(dir.path(), &root).unwrap();
        assert!(has_pending_root(dir.path()));

        let loaded = load_pending_root(dir.path()).expect("load pending root");
        assert_eq!(loaded.root_id(), root.root_id());
        // Signatures from loaded root verify under the original vk.
        let sig = loaded.sign(b"test message");
        assert!(root.verifying_key().verify(b"test message", &sig));

        clear_pending_root(dir.path());
        assert!(!has_pending_root(dir.path()));
        assert!(load_pending_root(dir.path()).is_none());
    }
}
