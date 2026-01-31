//! Escape hatches for advanced users.
//!
//! This module re-exports types from the underlying infrastructure
//! for users who need more control than the high-level SyncEngine provides.
//!
//! # When to Use
//!
//! Most applications should use the high-level SyncEngine types (`IndrasNetwork`,
//! `Realm`, `Document`, etc.). Use escape hatches when you need:
//!
//! - Custom protocol implementations
//! - Direct access to the sync layer
//! - Advanced cryptographic operations
//! - Custom storage backends
//!
//! # Example
//!
//! ```ignore
//! use indras_network::escape::*;
//!
//! // Access the underlying node
//! let node = network.node();
//!
//! // Register a custom protocol
//! node.transport().register_protocol("my-protocol", handler).await?;
//!
//! // Access raw storage
//! let storage = network.storage();
//! ```

// Re-export core types
pub use indras_core::{
    EventId, InterfaceId, PacketId, PeerIdentity, TopicId,
};

// Re-export node types
pub use indras_node::{
    IndrasNode, InviteKey, NodeConfig, NodeError,
};

// Re-export sync types
pub use indras_sync::{
    InterfaceDocument, NInterface, SyncState,
};

// Re-export traits from core
pub use indras_core::NInterfaceTrait;

// Re-export transport types
pub use indras_transport::{
    IrohIdentity, IrohNetworkAdapter,
};

// Re-export storage types
pub use indras_storage::{
    BlobStore, CompositeStorage, EventLog, StorageError,
};

// Re-export crypto types
pub use indras_crypto::{
    InterfaceKey, PQIdentity, PQKemKeyPair,
};

// Re-export messaging types
pub use indras_messaging::{
    Message as RawMessage, MessageContent, MessageHistory, MessagingClient,
};

/// Advanced realm configuration.
///
/// Use this when you need fine-grained control over realm behavior.
#[derive(Debug, Clone)]
pub struct RealmConfig {
    /// Human-readable name.
    pub name: String,
    /// Encryption mode.
    pub encryption: Encryption,
    /// Maximum number of members (None = unlimited).
    pub max_members: Option<usize>,
    /// Whether new members require an invite.
    pub invite_only: bool,
    /// Offline delivery settings.
    pub offline_delivery: OfflineDelivery,
}

impl Default for RealmConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            encryption: Encryption::Default,
            max_members: None,
            invite_only: true,
            offline_delivery: OfflineDelivery::default(),
        }
    }
}

/// Encryption mode for realm messages.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Encryption {
    /// Default encryption (ChaCha20-Poly1305 with X25519 key exchange).
    #[default]
    Default,
    /// Post-quantum encryption (ML-KEM key exchange).
    PostQuantum,
}

/// Offline delivery configuration.
#[derive(Debug, Clone)]
pub struct OfflineDelivery {
    /// Whether offline delivery is enabled.
    pub enabled: bool,
    /// Maximum age for stored messages.
    pub max_age: std::time::Duration,
    /// Maximum total size for stored messages.
    pub max_size_bytes: u64,
}

impl Default for OfflineDelivery {
    fn default() -> Self {
        Self {
            enabled: true,
            max_age: std::time::Duration::from_secs(7 * 24 * 60 * 60), // 7 days
            max_size_bytes: 100 * 1024 * 1024, // 100 MB
        }
    }
}
