//! Blob store implementation
//!
//! File-based content-addressed storage using BLAKE3 hashing.

use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use bytes::Bytes;
use tokio::fs::{self, File};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tracing::{debug, info, instrument, warn};

use super::content_ref::ContentRef;
use crate::error::StorageError;

/// Configuration for the blob store
#[derive(Debug, Clone)]
pub struct BlobStoreConfig {
    /// Base directory for blob storage
    pub base_dir: PathBuf,
    /// Number of subdirectory levels (for sharding)
    pub shard_depth: u8,
    /// Maximum blob size (bytes)
    pub max_blob_size: u64,
}

impl Default for BlobStoreConfig {
    fn default() -> Self {
        Self {
            base_dir: PathBuf::from("./data/blobs"),
            shard_depth: 2,                   // e.g., ab/cd/abcdef...
            max_blob_size: 100 * 1024 * 1024, // 100MB
        }
    }
}

/// Content-addressed blob store
pub struct BlobStore {
    config: BlobStoreConfig,
}

impl BlobStore {
    /// Create a new blob store
    pub async fn new(config: BlobStoreConfig) -> Result<Self, StorageError> {
        // Ensure base directory exists
        fs::create_dir_all(&config.base_dir)
            .await
            .map_err(|e| StorageError::Io(e.to_string()))?;

        info!(path = %config.base_dir.display(), "Blob store initialized");

        Ok(Self { config })
    }

    /// Store content and return its reference
    #[instrument(skip(self, data), fields(size = data.len()))]
    pub async fn store(&self, data: &[u8]) -> Result<ContentRef, StorageError> {
        if data.len() as u64 > self.config.max_blob_size {
            return Err(StorageError::CapacityExceeded);
        }

        let content_ref = ContentRef::from_data(data);

        // Check if already exists
        if self.exists(&content_ref).await? {
            debug!(hash = %content_ref.short_hash(), "Blob already exists");
            return Ok(content_ref);
        }

        let path = self.blob_path(&content_ref);

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|e| StorageError::Io(e.to_string()))?;
        }

        // Write atomically (write to temp, then rename)
        let temp_path = path.with_extension("tmp");

        let mut file = File::create(&temp_path)
            .await
            .map_err(|e| StorageError::Io(e.to_string()))?;

        file.write_all(data)
            .await
            .map_err(|e| StorageError::Io(e.to_string()))?;

        file.sync_all()
            .await
            .map_err(|e| StorageError::Io(e.to_string()))?;

        // Atomic rename
        fs::rename(&temp_path, &path)
            .await
            .map_err(|e| StorageError::Io(e.to_string()))?;

        debug!(hash = %content_ref.short_hash(), "Stored blob");
        Ok(content_ref)
    }

    /// Store content from Bytes
    pub async fn store_bytes(&self, data: Bytes) -> Result<ContentRef, StorageError> {
        self.store(&data).await
    }

    /// Load content by reference
    #[instrument(skip(self), fields(hash = %content_ref.short_hash()))]
    pub async fn load(&self, content_ref: &ContentRef) -> Result<Bytes, StorageError> {
        let path = self.blob_path(content_ref);

        let mut file = File::open(&path).await.map_err(|e| {
            if e.kind() == ErrorKind::NotFound {
                StorageError::PacketNotFound(content_ref.hash_hex())
            } else {
                StorageError::Io(e.to_string())
            }
        })?;

        let mut data = Vec::with_capacity(content_ref.size as usize);
        file.read_to_end(&mut data)
            .await
            .map_err(|e| StorageError::Io(e.to_string()))?;

        // Verify hash
        let actual_ref = ContentRef::from_data(&data);
        if !actual_ref.content_equals(content_ref) {
            warn!(
                expected = %content_ref.hash_hex(),
                actual = %actual_ref.hash_hex(),
                "Blob hash mismatch"
            );
            return Err(StorageError::Deserialization("Hash mismatch".into()));
        }

        Ok(Bytes::from(data))
    }

    /// Check if content exists
    pub async fn exists(&self, content_ref: &ContentRef) -> Result<bool, StorageError> {
        let path = self.blob_path(content_ref);
        Ok(path.exists())
    }

    /// Delete content by reference
    #[instrument(skip(self), fields(hash = %content_ref.short_hash()))]
    pub async fn delete(&self, content_ref: &ContentRef) -> Result<bool, StorageError> {
        let path = self.blob_path(content_ref);

        match fs::remove_file(&path).await {
            Ok(_) => {
                debug!("Deleted blob");
                Ok(true)
            }
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(false),
            Err(e) => Err(StorageError::Io(e.to_string())),
        }
    }

    /// Get the file path for a content reference
    fn blob_path(&self, content_ref: &ContentRef) -> PathBuf {
        let hash_hex = content_ref.hash_hex();

        let mut path = self.config.base_dir.clone();

        // Add shard directories
        for i in 0..self.config.shard_depth as usize {
            let start = i * 2;
            let end = start + 2;
            if end <= hash_hex.len() {
                path.push(&hash_hex[start..end]);
            }
        }

        // Add the full hash as filename
        path.push(&hash_hex);
        path
    }

    /// List all blobs (for debugging/maintenance)
    pub async fn list_all(&self) -> Result<Vec<ContentRef>, StorageError> {
        let mut refs = Vec::new();
        self.collect_blobs(&self.config.base_dir, &mut refs).await?;
        Ok(refs)
    }

    /// Recursively collect blob references
    fn collect_blobs<'a>(
        &'a self,
        dir: &'a Path,
        refs: &'a mut Vec<ContentRef>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), StorageError>> + Send + 'a>>
    {
        Box::pin(async move {
            let mut entries = fs::read_dir(dir)
                .await
                .map_err(|e| StorageError::Io(e.to_string()))?;

            while let Some(entry) = entries
                .next_entry()
                .await
                .map_err(|e| StorageError::Io(e.to_string()))?
            {
                let path = entry.path();

                if path.is_dir() {
                    self.collect_blobs(&path, refs).await?;
                } else if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    // Try to parse as hash
                    if name.len() == 64
                        && let Ok(hash_bytes) = hex::decode(name)
                        && hash_bytes.len() == 32
                    {
                        let mut hash = [0u8; 32];
                        hash.copy_from_slice(&hash_bytes);

                        let metadata = fs::metadata(&path)
                            .await
                            .map_err(|e| StorageError::Io(e.to_string()))?;

                        refs.push(ContentRef::new(hash, metadata.len()));
                    }
                }
            }

            Ok(())
        })
    }

    /// Get total size of all blobs
    pub async fn total_size(&self) -> Result<u64, StorageError> {
        let refs = self.list_all().await?;
        Ok(refs.iter().map(|r| r.size).sum())
    }

    /// Garbage collect unreferenced blobs
    /// Takes a closure that checks if a content ref is still referenced
    pub async fn gc<F>(&self, is_referenced: F) -> Result<GcResult, StorageError>
    where
        F: Fn(&ContentRef) -> bool,
    {
        let all_refs = self.list_all().await?;
        let mut result = GcResult::default();

        for content_ref in all_refs {
            if !is_referenced(&content_ref) {
                if self.delete(&content_ref).await? {
                    result.deleted_count += 1;
                    result.bytes_freed += content_ref.size;
                }
            } else {
                result.retained_count += 1;
            }
        }

        info!(
            deleted = result.deleted_count,
            retained = result.retained_count,
            bytes_freed = result.bytes_freed,
            "Garbage collection complete"
        );

        Ok(result)
    }
}

/// Result of garbage collection
#[derive(Debug, Default)]
pub struct GcResult {
    /// Number of blobs deleted
    pub deleted_count: usize,
    /// Number of blobs retained
    pub retained_count: usize,
    /// Bytes freed
    pub bytes_freed: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    async fn create_test_store() -> (BlobStore, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let config = BlobStoreConfig {
            base_dir: temp_dir.path().join("blobs"),
            ..Default::default()
        };
        let store = BlobStore::new(config).await.unwrap();
        (store, temp_dir)
    }

    #[tokio::test]
    async fn test_store_and_load() {
        let (store, _temp) = create_test_store().await;

        let data = b"Hello, blob storage!";
        let content_ref = store.store(data).await.unwrap();

        assert_eq!(content_ref.size, data.len() as u64);

        let loaded = store.load(&content_ref).await.unwrap();
        assert_eq!(&loaded[..], data);
    }

    #[tokio::test]
    async fn test_content_addressing() {
        let (store, _temp) = create_test_store().await;

        let data = b"Duplicate content";

        // Store twice
        let ref1 = store.store(data).await.unwrap();
        let ref2 = store.store(data).await.unwrap();

        // Should be the same reference
        assert!(ref1.content_equals(&ref2));

        // Only one file should exist
        let all = store.list_all().await.unwrap();
        assert_eq!(all.len(), 1);
    }

    #[tokio::test]
    async fn test_exists_and_delete() {
        let (store, _temp) = create_test_store().await;

        let data = b"Delete me";
        let content_ref = store.store(data).await.unwrap();

        assert!(store.exists(&content_ref).await.unwrap());

        let deleted = store.delete(&content_ref).await.unwrap();
        assert!(deleted);

        assert!(!store.exists(&content_ref).await.unwrap());

        // Delete again should return false
        let deleted = store.delete(&content_ref).await.unwrap();
        assert!(!deleted);
    }

    #[tokio::test]
    async fn test_hash_verification() {
        let (store, temp) = create_test_store().await;

        let data = b"Original data";
        let content_ref = store.store(data).await.unwrap();

        // Corrupt the file
        let path = temp
            .path()
            .join("blobs")
            .join(&content_ref.hash_hex()[0..2])
            .join(&content_ref.hash_hex()[2..4])
            .join(content_ref.hash_hex());

        fs::write(&path, b"Corrupted!").await.unwrap();

        // Load should fail due to hash mismatch
        let result = store.load(&content_ref).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_gc() {
        let (store, _temp) = create_test_store().await;

        // Store some blobs
        let ref1 = store.store(b"Keep me").await.unwrap();
        let ref2 = store.store(b"Delete me").await.unwrap();
        let ref3 = store.store(b"Keep me too").await.unwrap();

        // GC keeping only ref1 and ref3
        let result = store
            .gc(|r| r.content_equals(&ref1) || r.content_equals(&ref3))
            .await
            .unwrap();

        assert_eq!(result.deleted_count, 1);
        assert_eq!(result.retained_count, 2);

        assert!(store.exists(&ref1).await.unwrap());
        assert!(!store.exists(&ref2).await.unwrap());
        assert!(store.exists(&ref3).await.unwrap());
    }
}
