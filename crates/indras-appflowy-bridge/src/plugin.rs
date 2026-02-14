//! CollabPlugin implementation bridging AppFlowy to IndrasNetwork P2P
//!
//! For M1 we define a local mirror of AppFlowy-Collab's `CollabPlugin` trait.
//! In M2 this will be replaced with the real dependency.

use std::sync::Arc;

use indras_core::InterfaceId;
use indras_node::IndrasNode;
use tracing::{debug, info};
use yrs::{Doc, ReadTxn, StateVector, Transact};

use crate::envelope::AppFlowyEnvelope;
use crate::error::BridgeError;
use crate::id_mapping::WorkspaceMapping;
use crate::inbound::InboundListener;
use crate::outbound::OutboundForwarder;

/// Local mirror of AppFlowy-Collab's CollabPlugin trait.
///
/// In M2 this will be replaced with the real `collab::preclude::CollabPlugin`.
pub trait CollabPlugin: Send + Sync {
    /// Called once when the Collab is initialized. The plugin receives a
    /// clone of the Yrs Doc (which is internally Arc'd).
    fn init(&self, object_id: &str, doc: Doc);

    /// Called synchronously whenever the local user makes an edit.
    /// `update` is a Yrs v1-encoded update.
    fn receive_local_update(&self, object_id: &str, update: &[u8]);

    /// Called to push the full local state to the network (e.g. on open).
    fn receive_local_state(&self, object_id: &str, doc: &Doc);
}

/// Configuration for the IndrasNetwork bridge plugin.
pub struct BridgeConfig {
    /// The shared workspace seed (32 bytes, distributed via invite).
    pub workspace_seed: [u8; 32],
    /// Bootstrap peers for gossip discovery (iroh PublicKeys).
    pub bootstrap_peers: Vec<iroh::PublicKey>,
}

/// The bridge plugin that implements CollabPlugin.
///
/// Each instance handles one AppFlowy workspace. Multiple documents within
/// the workspace are multiplexed over different InterfaceIds derived from
/// the workspace seed.
pub struct IndrasNetworkPlugin {
    node: Arc<IndrasNode>,
    mapping: WorkspaceMapping,
    bootstrap_peers: Vec<iroh::PublicKey>,
    /// Outbound forwarder (created on first init)
    outbound: OutboundForwarder,
    /// Active inbound listeners keyed by object_id
    inbound_listeners: dashmap::DashMap<String, InboundListener>,
}

impl IndrasNetworkPlugin {
    /// Create a new bridge plugin.
    ///
    /// The plugin will use the given IndrasNode for all P2P communication.
    /// `config.workspace_seed` determines how object IDs map to InterfaceIds.
    pub fn new(node: Arc<IndrasNode>, config: BridgeConfig) -> Self {
        let outbound = OutboundForwarder::spawn(Arc::clone(&node));
        Self {
            node,
            mapping: WorkspaceMapping::new(config.workspace_seed),
            bootstrap_peers: config.bootstrap_peers,
            outbound,
            inbound_listeners: dashmap::DashMap::new(),
        }
    }

    /// Get the InterfaceId for an object.
    pub fn interface_id_for(&self, object_id: &str) -> InterfaceId {
        self.mapping.interface_id(object_id)
    }

    /// Ensure the interface exists on the node. Creates it if necessary.
    async fn ensure_interface(&self, object_id: &str) -> Result<InterfaceId, BridgeError> {
        let interface_id = self.mapping.interface_id(object_id);
        let key_seed = self.mapping.key_seed(object_id);

        self.node
            .create_interface_with_seed(
                interface_id,
                &key_seed,
                Some(object_id),
                self.bootstrap_peers.clone(),
            )
            .await
            .map_err(|e| BridgeError::Node(e.to_string()))?;

        Ok(interface_id)
    }
}

impl CollabPlugin for IndrasNetworkPlugin {
    fn init(&self, object_id: &str, doc: Doc) {
        let interface_id = self.mapping.interface_id(object_id);
        let key_seed = self.mapping.key_seed(object_id);
        let object_id_hash = AppFlowyEnvelope::hash_object_id(object_id);

        // Create the interface on the node (fire-and-forget from sync context)
        let node = Arc::clone(&self.node);
        let bootstrap = self.bootstrap_peers.clone();
        let obj_id = object_id.to_string();
        tokio::spawn(async move {
            if let Err(e) = node
                .create_interface_with_seed(
                    interface_id,
                    &key_seed,
                    Some(&obj_id),
                    bootstrap,
                )
                .await
            {
                tracing::error!(error = %e, object_id = %obj_id, "failed to create interface during init");
            }
        });

        // Start inbound listener
        match InboundListener::spawn(
            Arc::clone(&self.node),
            interface_id,
            doc,
            object_id_hash,
        ) {
            Ok(listener) => {
                self.inbound_listeners
                    .insert(object_id.to_string(), listener);
                info!(object_id, %interface_id, "bridge initialized for object");
            }
            Err(e) => {
                tracing::error!(error = %e, object_id, "failed to start inbound listener");
            }
        }
    }

    fn receive_local_update(&self, object_id: &str, update: &[u8]) {
        let interface_id = self.mapping.interface_id(object_id);
        let object_id_hash = AppFlowyEnvelope::hash_object_id(object_id);

        if let Err(e) = self.outbound.send(interface_id, object_id_hash, update.to_vec()) {
            tracing::error!(error = %e, object_id, "failed to queue outbound update");
        } else {
            debug!(object_id, update_len = update.len(), "queued local update for outbound");
        }
    }

    fn receive_local_state(&self, object_id: &str, doc: &Doc) {
        let interface_id = self.mapping.interface_id(object_id);
        let object_id_hash = AppFlowyEnvelope::hash_object_id(object_id);

        // Encode full state as a Yrs update from empty state vector
        let txn = doc.transact();
        let full_update = txn.encode_state_as_update_v1(&StateVector::default());

        if let Err(e) = self.outbound.send(interface_id, object_id_hash, full_update) {
            tracing::error!(error = %e, object_id, "failed to send local state");
        } else {
            info!(object_id, "sent full local state to network");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bridge_config_creation() {
        let config = BridgeConfig {
            workspace_seed: [0x42u8; 32],
            bootstrap_peers: vec![],
        };
        assert_eq!(config.workspace_seed, [0x42u8; 32]);
        assert!(config.bootstrap_peers.is_empty());
    }

    #[test]
    fn test_collab_plugin_trait_is_object_safe() {
        // Verify the trait can be used as a trait object
        fn _assert_object_safe(_: &dyn CollabPlugin) {}
    }
}
