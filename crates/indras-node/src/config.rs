//! Configuration for the node coordinator

use std::path::PathBuf;

use indras_storage::CompositeStorageConfig;
use indras_transport::AdapterConfig;

/// Configuration for an IndrasNode
#[derive(Debug, Clone)]
pub struct NodeConfig {
    /// Base directory for all node data
    pub data_dir: PathBuf,
    /// Transport adapter configuration
    pub transport: AdapterConfig,
    /// Storage configuration
    pub storage: CompositeStorageConfig,
    /// Event broadcast channel capacity
    pub event_channel_capacity: usize,
    /// Allow unsigned (legacy) messages
    ///
    /// When true (default during transition), accepts unsigned messages with a warning.
    /// Set to false in production to enforce PQ signatures on all messages.
    pub allow_legacy_unsigned: bool,
}

impl Default for NodeConfig {
    fn default() -> Self {
        let data_dir = PathBuf::from("./indras-data");
        Self {
            data_dir: data_dir.clone(),
            transport: AdapterConfig::default(),
            storage: CompositeStorageConfig::with_base_dir(data_dir.join("storage")),
            event_channel_capacity: 1024,
            // Default to allowing legacy during transition period
            allow_legacy_unsigned: true,
        }
    }
}

impl NodeConfig {
    /// Create a configuration with a custom data directory
    pub fn with_data_dir(data_dir: impl Into<PathBuf>) -> Self {
        let data_dir = data_dir.into();
        Self {
            data_dir: data_dir.clone(),
            transport: AdapterConfig::default(),
            storage: CompositeStorageConfig::with_base_dir(data_dir.join("storage")),
            event_channel_capacity: 1024,
            // Default to allowing legacy during transition period
            allow_legacy_unsigned: true,
        }
    }

    /// Set the transport configuration
    pub fn with_transport(mut self, transport: AdapterConfig) -> Self {
        self.transport = transport;
        self
    }

    /// Set the storage configuration
    pub fn with_storage(mut self, storage: CompositeStorageConfig) -> Self {
        self.storage = storage;
        self
    }

    /// Set the event channel capacity
    pub fn with_event_channel_capacity(mut self, capacity: usize) -> Self {
        self.event_channel_capacity = capacity;
        self
    }

    /// Configure whether to allow unsigned (legacy) messages
    ///
    /// When false, unsigned messages will be rejected with an error.
    /// Set to false in production to enforce PQ signatures on all messages.
    ///
    /// Default is true during the transition period.
    pub fn with_allow_legacy_unsigned(mut self, allow: bool) -> Self {
        self.allow_legacy_unsigned = allow;
        self
    }

    /// Disable legacy unsigned messages (enforce PQ signatures)
    ///
    /// This is a convenience method equivalent to `.with_allow_legacy_unsigned(false)`.
    /// Use this in production deployments.
    pub fn enforce_pq_signatures(self) -> Self {
        self.with_allow_legacy_unsigned(false)
    }
}
