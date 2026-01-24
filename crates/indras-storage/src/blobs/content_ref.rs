//! Content reference types

use serde::{Deserialize, Serialize};

/// Reference to content-addressed blob
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ContentRef {
    /// BLAKE3 hash of the content
    pub hash: [u8; 32],
    /// Size of the content in bytes
    pub size: u64,
}

impl ContentRef {
    /// Create a new content reference
    pub fn new(hash: [u8; 32], size: u64) -> Self {
        Self { hash, size }
    }

    /// Compute a content reference from data
    pub fn from_data(data: &[u8]) -> Self {
        let hash = blake3::hash(data);
        Self {
            hash: *hash.as_bytes(),
            size: data.len() as u64,
        }
    }

    /// Get the hash as a hex string
    pub fn hash_hex(&self) -> String {
        hex::encode(self.hash)
    }

    /// Get a short hash for display (first 8 chars)
    pub fn short_hash(&self) -> String {
        hex::encode(&self.hash[..4])
    }

    /// Check if two content refs are equal (based on hash)
    pub fn content_equals(&self, other: &ContentRef) -> bool {
        self.hash == other.hash
    }

    /// Size threshold for storing in blobs vs inline
    /// Content smaller than this should be stored inline
    pub const INLINE_THRESHOLD: u64 = 4096; // 4KB

    /// Check if this content should be stored in blob storage
    pub fn should_store_as_blob(&self) -> bool {
        self.size >= Self::INLINE_THRESHOLD
    }
}

impl std::fmt::Display for ContentRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ContentRef({}, {} bytes)", self.short_hash(), self.size)
    }
}

/// Metadata about stored content
#[allow(dead_code)] // Reserved for future garbage collection feature
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentMetadata {
    /// The content reference
    pub content_ref: ContentRef,
    /// When the content was stored (Unix millis)
    pub stored_at_millis: i64,
    /// Content type hint (e.g., "automerge/snapshot", "application/octet-stream")
    pub content_type: Option<String>,
    /// Reference count (for garbage collection)
    pub ref_count: u32,
    /// Optional tags for organization
    pub tags: Vec<String>,
}

#[allow(dead_code)] // Reserved for future garbage collection feature
impl ContentMetadata {
    /// Create new metadata for content
    pub fn new(content_ref: ContentRef) -> Self {
        Self {
            content_ref,
            stored_at_millis: chrono::Utc::now().timestamp_millis(),
            content_type: None,
            ref_count: 1,
            tags: Vec::new(),
        }
    }

    /// Set content type
    pub fn with_content_type(mut self, content_type: impl Into<String>) -> Self {
        self.content_type = Some(content_type.into());
        self
    }

    /// Add a tag
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_content_ref_from_data() {
        let data = b"Hello, world!";
        let content_ref = ContentRef::from_data(data);

        assert_eq!(content_ref.size, data.len() as u64);
        assert!(!content_ref.hash_hex().is_empty());

        // Same data should produce same hash
        let content_ref2 = ContentRef::from_data(data);
        assert!(content_ref.content_equals(&content_ref2));

        // Different data should produce different hash
        let content_ref3 = ContentRef::from_data(b"Different data");
        assert!(!content_ref.content_equals(&content_ref3));
    }

    #[test]
    fn test_inline_threshold() {
        let small_data = vec![0u8; 100];
        let small_ref = ContentRef::from_data(&small_data);
        assert!(!small_ref.should_store_as_blob());

        let large_data = vec![0u8; 10000];
        let large_ref = ContentRef::from_data(&large_data);
        assert!(large_ref.should_store_as_blob());
    }

    #[test]
    fn test_display() {
        let content_ref = ContentRef::from_data(b"test");
        let display = format!("{}", content_ref);
        assert!(display.contains("ContentRef"));
        assert!(display.contains("bytes"));
    }
}
