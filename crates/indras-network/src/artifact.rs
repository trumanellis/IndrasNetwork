//! Artifact - static, immutable content sharing.
//!
//! Artifacts are files or binary content shared within a realm.
//! Unlike documents, artifacts are immutable once shared and are
//! content-addressed by their hash.

use crate::access::ArtifactStatus;
use crate::error::{IndraError, Result};
use crate::member::{Member, MemberId};

use chrono::{DateTime, Utc};
use futures::Stream;
use std::path::PathBuf;
use tokio::sync::watch;

/// Unique identifier for an artifact.
///
/// Re-exported from `indras_artifacts`. This is an enum with `Blob` and `Doc` variants,
/// each wrapping a `[u8; 32]` hash.
pub use indras_artifacts::ArtifactId;

/// A shared artifact (file or binary content).
///
/// Artifacts are immutable - once shared, their content never changes.
/// They are identified by their content hash (BLAKE3).
#[derive(Debug, Clone)]
pub struct Artifact {
    /// Content hash (BLAKE3), wrapped in ArtifactId::Blob.
    pub id: ArtifactId,
    /// Original filename.
    pub name: String,
    /// Size in bytes.
    pub size: u64,
    /// MIME type if known.
    pub mime_type: Option<String>,
    /// Who shared this artifact.
    pub sharer: Member,
    /// Steward (owner) of the artifact.
    pub steward: MemberId,
    /// When it was shared.
    pub shared_at: DateTime<Utc>,
    /// Whether content is per-artifact encrypted (for revocable sharing).
    pub is_encrypted: bool,
    /// Current lifecycle status.
    pub status: ArtifactStatus,
    /// Parent artifact this is a part of (None if top-level).
    pub parent: Option<ArtifactId>,
    /// Child artifact IDs composing this holon.
    pub children: Vec<ArtifactId>,
}

impl Artifact {
    /// Get the artifact's ticket string for sharing.
    pub fn ticket(&self) -> String {
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
        URL_SAFE_NO_PAD.encode(self.id.bytes())
    }

    /// Get the content hash as a hex string.
    pub fn hash_hex(&self) -> String {
        self.id.bytes().iter().map(|b| format!("{:02x}", b)).collect()
    }

    /// Check if this artifact is accessible (active status).
    pub fn is_accessible(&self) -> bool {
        self.status.is_active()
    }

    /// Check if this artifact has been recalled.
    pub fn is_recalled(&self) -> bool {
        matches!(self.status, ArtifactStatus::Recalled { .. })
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
pub struct ArtifactDownload {
    /// The artifact being downloaded.
    artifact: Artifact,
    /// Progress receiver.
    progress_rx: watch::Receiver<DownloadProgress>,
    /// Destination path.
    destination: PathBuf,
    /// Cancellation signal sender.
    cancel_tx: watch::Sender<bool>,
}

impl ArtifactDownload {
    /// Create a new download handle.
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

        let mut progress_rx = self.progress_rx.clone();
        let mut cancel_rx = self.cancel_tx.subscribe();

        loop {
            if progress_rx.borrow().is_complete() {
                return Ok(self.destination);
            }

            tokio::select! {
                result = progress_rx.changed() => {
                    if result.is_err() {
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
    pub fn cancel(&self) {
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
        let identity = indras_transport::IrohIdentity::from_array([1u8; 32])
            .expect("valid test identity");

        let artifact = Artifact {
            id: ArtifactId::Blob([0u8; 32]),
            name: "test.txt".to_string(),
            size: 100,
            mime_type: Some("text/plain".to_string()),
            sharer: Member::new(identity),
            steward: [1u8; 32],
            shared_at: Utc::now(),
            is_encrypted: false,
            status: ArtifactStatus::Active,
            parent: None,
            children: Vec::new(),
        };

        assert!(!artifact.ticket().is_empty());
        assert_eq!(artifact.hash_hex().len(), 64);
        assert!(artifact.is_accessible());
        assert!(!artifact.is_recalled());
    }

    #[test]
    fn test_artifact_recalled_status() {
        let identity = indras_transport::IrohIdentity::from_array([1u8; 32])
            .expect("valid test identity");

        let artifact = Artifact {
            id: ArtifactId::Blob([0u8; 32]),
            name: "test.txt".to_string(),
            size: 100,
            mime_type: Some("text/plain".to_string()),
            sharer: Member::new(identity),
            steward: [1u8; 32],
            shared_at: Utc::now(),
            is_encrypted: true,
            status: ArtifactStatus::Recalled { recalled_at: 12345 },
            parent: None,
            children: Vec::new(),
        };

        assert!(!artifact.is_accessible());
        assert!(artifact.is_recalled());
        assert!(artifact.requires_decryption());
    }
}
