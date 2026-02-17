//! Artifact - static, immutable content sharing.
//!
//! Artifacts are files or binary content shared within a realm.
//! Unlike documents, artifacts are immutable once shared and are
//! content-addressed by their hash.

use crate::error::{IndraError, Result};

use futures::Stream;
use std::path::PathBuf;
use tokio::sync::watch;

/// Unique identifier for an artifact.
///
/// Re-exported from `indras_artifacts`. This is an enum with `Blob` and `Doc` variants,
/// each wrapping a `[u8; 32]` hash.
pub use indras_artifacts::ArtifactId;
pub use indras_artifacts::{generate_tree_id, leaf_id, dm_story_id};

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
    /// The artifact ID being downloaded.
    artifact_id: ArtifactId,
    /// The artifact name.
    name: String,
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
        artifact_id: ArtifactId,
        name: String,
        progress_rx: watch::Receiver<DownloadProgress>,
        destination: PathBuf,
    ) -> (Self, watch::Receiver<bool>) {
        let (cancel_tx, cancel_rx) = watch::channel(false);
        let download = Self {
            artifact_id,
            name,
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

    /// Get the artifact ID being downloaded.
    pub fn artifact_id(&self) -> &ArtifactId {
        &self.artifact_id
    }

    /// Get the artifact name.
    pub fn name(&self) -> &str {
        &self.name
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
            .field("artifact", &self.name)
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
}
