//! Artifact - static, immutable content sharing.
//!
//! Artifacts are files or binary content shared within a realm.
//! Unlike documents, artifacts are immutable once shared and are
//! content-addressed by their hash.
//!
//! ## Revocable Sharing
//!
//! Artifacts can be shared with revocation support. When shared revocably:
//! - Content is encrypted with a per-artifact key
//! - The key can be deleted to revoke access
//! - Recalling an artifact creates a tombstone in chat
//!
//! See [`crate::artifact_sharing`] for the revocation system.

use crate::artifact_sharing::SharingStatus;
use crate::error::{IndraError, Result};
use crate::member::{Member, MemberId};

use chrono::{DateTime, Utc};
use futures::Stream;
use std::path::PathBuf;
use tokio::sync::watch;

/// Unique identifier for an artifact (BLAKE3 hash).
pub type ArtifactId = [u8; 32];

/// A shared artifact (file or binary content).
///
/// Artifacts are immutable - once shared, their content never changes.
/// They are identified by their content hash (BLAKE3).
///
/// ## Revocable Sharing
///
/// When `is_encrypted` is true, the artifact content is encrypted with
/// a per-artifact key. The `sharing_status` indicates whether the artifact
/// is still accessible or has been recalled.
///
/// # Example
///
/// ```ignore
/// // Share an artifact
/// let artifact = realm.share_artifact("./document.pdf").await?;
/// println!("Shared: {} ({} bytes)", artifact.name, artifact.size);
///
/// // Download an artifact
/// let download = realm.download(&artifact).await?;
/// let path = download.finish().await?;
/// ```
#[derive(Debug, Clone)]
pub struct Artifact {
    /// Content hash (BLAKE3).
    pub id: ArtifactId,
    /// Original filename.
    pub name: String,
    /// Size in bytes.
    pub size: u64,
    /// MIME type if known.
    pub mime_type: Option<String>,
    /// Who shared this artifact.
    pub sharer: Member,
    /// Owner of the artifact.
    pub owner: MemberId,
    /// When it was shared.
    pub shared_at: DateTime<Utc>,
    /// Whether content is per-artifact encrypted (for revocable sharing).
    pub is_encrypted: bool,
    /// Current sharing status (Shared or Recalled).
    pub sharing_status: SharingStatus,
    /// Parent artifact this is a part of (None if top-level).
    pub parent: Option<ArtifactId>,
    /// Child artifact IDs composing this holon.
    pub children: Vec<ArtifactId>,
}

impl Artifact {
    /// Get the artifact's ticket string for sharing.
    ///
    /// This can be used to reference the artifact outside of the realm.
    pub fn ticket(&self) -> String {
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
        URL_SAFE_NO_PAD.encode(self.id)
    }

    /// Get the content hash as a hex string.
    pub fn hash_hex(&self) -> String {
        self.id.iter().map(|b| format!("{:02x}", b)).collect()
    }

    /// Check if this artifact is accessible (not recalled).
    pub fn is_accessible(&self) -> bool {
        self.sharing_status.is_shared()
    }

    /// Check if this artifact has been recalled.
    pub fn is_recalled(&self) -> bool {
        self.sharing_status.is_recalled()
    }

    /// Check if this artifact requires decryption.
    pub fn requires_decryption(&self) -> bool {
        self.is_encrypted
    }
}

/// Progress of an artifact download.
#[derive(Debug, Clone, Copy)]
pub struct DownloadProgress {
    /// Bytes downloaded so far.
    pub bytes_downloaded: u64,
    /// Total bytes to download.
    pub total_bytes: u64,
}

impl DownloadProgress {
    /// Get the download progress as a percentage (0.0 - 100.0).
    pub fn percent(&self) -> f32 {
        if self.total_bytes == 0 {
            100.0
        } else {
            (self.bytes_downloaded as f32 / self.total_bytes as f32) * 100.0
        }
    }

    /// Check if the download is complete.
    pub fn is_complete(&self) -> bool {
        self.bytes_downloaded >= self.total_bytes
    }
}

/// Handle for an in-progress artifact download.
///
/// # Example
///
/// ```ignore
/// let download = realm.download(&artifact).await?;
///
/// // Track progress
/// let mut progress = download.progress();
/// while let Some(p) = progress.next().await {
///     println!("{}%", p.percent() as u32);
///     if p.is_complete() {
///         break;
///     }
/// }
///
/// // Get the downloaded file
/// let path = download.finish().await?;
/// println!("Saved to: {}", path.display());
/// ```
pub struct ArtifactDownload {
    /// The artifact being downloaded.
    artifact: Artifact,
    /// Progress receiver.
    progress_rx: watch::Receiver<DownloadProgress>,
    /// Destination path.
    destination: PathBuf,
    /// Cancellation signal sender. When `true` is sent, the download task should stop.
    cancel_tx: watch::Sender<bool>,
}

impl ArtifactDownload {
    /// Create a new download handle.
    ///
    /// Returns both the download handle and a cancellation receiver that the
    /// download task should monitor. When the receiver yields `true`, the
    /// download task should stop.
    pub(crate) fn new(
        artifact: Artifact,
        progress_rx: watch::Receiver<DownloadProgress>,
        destination: PathBuf,
    ) -> (Self, watch::Receiver<bool>) {
        let (cancel_tx, cancel_rx) = watch::channel(false);
        let download = Self {
            artifact,
            progress_rx,
            destination,
            cancel_tx,
        };
        (download, cancel_rx)
    }

    /// Check if this download has been cancelled.
    pub fn is_cancelled(&self) -> bool {
        *self.cancel_tx.borrow()
    }

    /// Get the artifact being downloaded.
    pub fn artifact(&self) -> &Artifact {
        &self.artifact
    }

    /// Get the current progress.
    pub fn current_progress(&self) -> DownloadProgress {
        *self.progress_rx.borrow()
    }

    /// Get a stream of progress updates.
    pub fn progress(&self) -> impl Stream<Item = DownloadProgress> + '_ {
        async_stream::stream! {
            let mut rx = self.progress_rx.clone();
            while rx.changed().await.is_ok() {
                yield *rx.borrow();
            }
        }
    }

    /// Wait for the download to complete and return the file path.
    pub async fn finish(self) -> Result<PathBuf> {
        if self.is_cancelled() {
            return Err(IndraError::Artifact("Download was cancelled".to_string()));
        }

        // Wait for completion
        let mut progress_rx = self.progress_rx.clone();
        let mut cancel_rx = self.cancel_tx.subscribe();

        loop {
            if progress_rx.borrow().is_complete() {
                return Ok(self.destination);
            }

            tokio::select! {
                result = progress_rx.changed() => {
                    if result.is_err() {
                        // Progress channel closed - check if cancelled
                        if *self.cancel_tx.borrow() {
                            return Err(IndraError::Artifact("Download was cancelled".to_string()));
                        }
                        return Err(IndraError::Artifact("Download failed".to_string()));
                    }
                }
                _ = cancel_rx.changed() => {
                    if *cancel_rx.borrow() {
                        return Err(IndraError::Artifact("Download was cancelled".to_string()));
                    }
                }
            }
        }
    }

    /// Cancel the download.
    ///
    /// This signals the download task to stop. The task should be monitoring
    /// the cancellation receiver returned from `new()` and stop when it
    /// receives `true`.
    pub fn cancel(&self) {
        // Signal cancellation to the download task.
        // The send only fails if there are no receivers, which is fine.
        let _ = self.cancel_tx.send(true);
    }
}

impl std::fmt::Debug for ArtifactDownload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ArtifactDownload")
            .field("artifact", &self.artifact.name)
            .field("progress", &self.current_progress())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_download_progress() {
        let progress = DownloadProgress {
            bytes_downloaded: 50,
            total_bytes: 100,
        };
        assert_eq!(progress.percent(), 50.0);
        assert!(!progress.is_complete());

        let complete = DownloadProgress {
            bytes_downloaded: 100,
            total_bytes: 100,
        };
        assert_eq!(complete.percent(), 100.0);
        assert!(complete.is_complete());
    }

    #[test]
    fn test_artifact_ticket() {
        // Create a test identity from zeroed bytes
        let identity = indras_transport::IrohIdentity::from_array([1u8; 32])
            .expect("valid test identity");

        let artifact = Artifact {
            id: [0u8; 32],
            name: "test.txt".to_string(),
            size: 100,
            mime_type: Some("text/plain".to_string()),
            sharer: Member::new(identity),
            owner: [1u8; 32],
            shared_at: Utc::now(),
            is_encrypted: false,
            sharing_status: SharingStatus::Shared,
            parent: None,
            children: Vec::new(),
        };

        assert!(!artifact.ticket().is_empty());
        assert_eq!(artifact.hash_hex().len(), 64); // 32 bytes * 2 hex chars
        assert!(artifact.is_accessible());
        assert!(!artifact.is_recalled());
    }

    #[test]
    fn test_artifact_recalled_status() {
        let identity = indras_transport::IrohIdentity::from_array([1u8; 32])
            .expect("valid test identity");

        let artifact = Artifact {
            id: [0u8; 32],
            name: "test.txt".to_string(),
            size: 100,
            mime_type: Some("text/plain".to_string()),
            sharer: Member::new(identity),
            owner: [1u8; 32],
            shared_at: Utc::now(),
            is_encrypted: true,
            sharing_status: SharingStatus::Recalled {
                recalled_at: 12345,
                recalled_by: "abc123".to_string(),
            },
            parent: None,
            children: Vec::new(),
        };

        assert!(!artifact.is_accessible());
        assert!(artifact.is_recalled());
        assert!(artifact.requires_decryption());
    }
}
