//! Manifest materialization: write files from a [`PatchManifest`] to disk.

use std::path::Path;
use std::sync::Arc;

use indras_storage::{BlobStore, ContentRef};

use crate::braid::changeset::PatchManifest;

/// Write every file described by `manifest` into `dest`, fetching blobs from
/// `blob_store`.
///
/// For each [`PatchFile`](crate::braid::changeset::PatchFile) in the manifest:
/// 1. A [`ContentRef`] is constructed from the file's hash and size.
/// 2. The blob is loaded from `blob_store`.
/// 3. Parent directories under `dest` are created as needed.
/// 4. The bytes are written to `dest/<path>`.
///
/// This is a thin, vault-free counterpart to
/// [`Vault::apply_manifest`](crate::vault::Vault::apply_manifest).  It
/// provides a stable API surface for the `project` module so that callers
/// remain insulated from vault internals — if the vault's checkout
/// machinery ever changes, this wrapper stays unchanged.
///
/// # Errors
///
/// Returns an `std::io::Error`-based error if any blob is missing from the
/// store or if any file write fails.
pub async fn materialize_to(
    manifest: &PatchManifest,
    dest: &Path,
    blob_store: &Arc<BlobStore>,
) -> Result<(), std::io::Error> {
    for pf in &manifest.files {
        let content_ref = ContentRef::new(pf.hash, pf.size);
        let data = blob_store.load(&content_ref).await.map_err(|e| {
            std::io::Error::other(format!(
                "materialize_to: blob for {} not available: {e}",
                pf.path
            ))
        })?;

        let full_path = dest.join(&pf.path);
        if let Some(parent) = full_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&full_path, &data).await?;
    }
    Ok(())
}
