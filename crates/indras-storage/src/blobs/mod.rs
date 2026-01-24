//! Content-addressed blob storage
//!
//! This module provides content-addressed storage for large payloads,
//! document snapshots, and attachments.
//!
//! Uses BLAKE3 for hashing and file-based storage.

mod content_ref;
mod store;

pub use content_ref::ContentRef;
pub use store::{BlobStore, BlobStoreConfig};
