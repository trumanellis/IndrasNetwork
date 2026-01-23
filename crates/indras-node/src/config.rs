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
}

impl Default for NodeConfig {
    fn default() -> Self {
        let data_dir = PathBuf::from("./indras-data");
        Self {
            data_dir: data_dir.clone(),
            transport: AdapterConfig::default(),
            storage: CompositeStorageConfig::with_base_dir(data_dir.join("storage")),
            event_channel_capacity: 1024,
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
}
