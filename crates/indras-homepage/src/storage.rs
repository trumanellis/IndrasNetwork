//! File-based artifact storage with metadata sidecars.
//!
//! Each artifact is stored as two files: the raw data at `{hex_id}` and
//! a JSON metadata sidecar at `{hex_id}.meta.json`. Storage limits are
//! enforced per-file and in aggregate.

use std::path::{Path, PathBuf};
use tokio::fs;

/// Configuration for artifact storage limits.
pub struct StorageConfig {
    /// Maximum size per artifact in bytes (default 10 MB).
    pub max_file_size: u64,
    /// Maximum total storage in bytes (default 100 MB).
    pub max_total_size: u64,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            max_file_size: 10 * 1024 * 1024,
            max_total_size: 100 * 1024 * 1024,
        }
    }
}

/// Metadata sidecar for a stored artifact.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ArtifactMeta {
    /// Human-readable artifact name.
    pub name: String,
    /// MIME type if known.
    pub mime_type: Option<String>,
    /// Size in bytes.
    pub size: u64,
    /// Creation timestamp (Unix epoch seconds).
    pub created_at: i64,
}

/// File-based artifact store.
pub struct ArtifactStore {
    base_dir: PathBuf,
    config: StorageConfig,
}

impl ArtifactStore {
    /// Create a new artifact store, creating the base directory if needed.
    pub async fn new(base_dir: impl AsRef<Path>) -> std::io::Result<Self> {
        let base_dir = base_dir.as_ref().to_path_buf();
        fs::create_dir_all(&base_dir).await?;
        Ok(Self {
            base_dir,
            config: StorageConfig::default(),
        })
    }

    /// Override the default storage configuration.
    pub fn with_config(mut self, config: StorageConfig) -> Self {
        self.config = config;
        self
    }

    fn artifact_path(&self, id: &[u8; 32]) -> PathBuf {
        self.base_dir.join(hex::encode(id))
    }

    fn meta_path(&self, id: &[u8; 32]) -> PathBuf {
        self.base_dir.join(format!("{}.meta.json", hex::encode(id)))
    }

    /// Store an artifact's data and metadata.
    pub async fn store(
        &self,
        id: &[u8; 32],
        data: &[u8],
        meta: &ArtifactMeta,
    ) -> Result<(), StorageError> {
        if data.len() as u64 > self.config.max_file_size {
            return Err(StorageError::FileTooLarge);
        }
        let current_total = self.total_size().await;
        if current_total + data.len() as u64 > self.config.max_total_size {
            return Err(StorageError::StorageFull);
        }
        fs::write(self.artifact_path(id), data)
            .await
            .map_err(StorageError::Io)?;
        let meta_json =
            serde_json::to_string_pretty(meta).map_err(|e| StorageError::Io(std::io::Error::other(e)))?;
        fs::write(self.meta_path(id), meta_json)
            .await
            .map_err(StorageError::Io)?;
        Ok(())
    }

    /// Load an artifact's raw data.
    pub async fn load(&self, id: &[u8; 32]) -> Result<Vec<u8>, StorageError> {
        fs::read(self.artifact_path(id)).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                StorageError::NotFound
            } else {
                StorageError::Io(e)
            }
        })
    }

    /// Load an artifact's metadata sidecar.
    pub async fn load_meta(&self, id: &[u8; 32]) -> Result<ArtifactMeta, StorageError> {
        let data = fs::read(self.meta_path(id)).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                StorageError::NotFound
            } else {
                StorageError::Io(e)
            }
        })?;
        serde_json::from_slice(&data).map_err(|e| StorageError::Io(std::io::Error::other(e)))
    }

    /// Delete an artifact and its metadata sidecar.
    pub async fn delete(&self, id: &[u8; 32]) -> Result<(), StorageError> {
        let _ = fs::remove_file(self.artifact_path(id)).await;
        let _ = fs::remove_file(self.meta_path(id)).await;
        Ok(())
    }

    /// Sum the size of all files in the store directory.
    async fn total_size(&self) -> u64 {
        let mut total = 0u64;
        if let Ok(mut entries) = fs::read_dir(&self.base_dir).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                if let Ok(meta) = entry.metadata().await {
                    total += meta.len();
                }
            }
        }
        total
    }
}

/// Errors from artifact storage operations.
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    /// The requested artifact was not found.
    #[error("artifact not found")]
    NotFound,
    /// A single file exceeds the maximum allowed size.
    #[error("file exceeds maximum size")]
    FileTooLarge,
    /// Total storage quota would be exceeded.
    #[error("storage quota exceeded")]
    StorageFull,
    /// Underlying I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}
