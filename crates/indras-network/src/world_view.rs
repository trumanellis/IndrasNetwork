//! World View — snapshot of this node's view of the network.
//!
//! When the app quits, each instance writes a JSON "save file" capturing
//! its view of the world: identity, peers, interfaces, members, and
//! connection state. Comparing these files across instances reveals
//! sync discrepancies.

use serde::Serialize;
use std::path::Path;

use indras_core::transport::Transport;
use indras_core::PeerIdentity;
use indras_transport::IrohIdentity;

use crate::chat_message::RealmChatDocument;
use crate::error::{IndraError, Result};
use crate::network::IndrasNetwork;

/// Complete snapshot of this node's view of the network.
#[derive(Debug, Serialize)]
pub struct WorldView {
    pub timestamp: String,
    pub node: NodeInfo,
    pub interfaces: Vec<InterfaceInfo>,
    pub peers: Vec<PeerViewInfo>,
    pub transport: TransportInfo,
}

/// Identity and endpoint info for this node.
#[derive(Debug, Serialize)]
pub struct NodeInfo {
    pub display_name: Option<String>,
    pub iroh_public_key: String,
    pub member_id: String,
    pub endpoint_addr: Option<String>,
    pub data_dir: String,
}

/// A single interface (realm) and its members.
#[derive(Debug, Serialize)]
pub struct InterfaceInfo {
    pub id: String,
    pub name: Option<String>,
    pub event_count: u64,
    pub member_count: u32,
    pub encrypted: bool,
    pub created_at_millis: i64,
    pub last_activity_millis: i64,
    pub members: Vec<MemberViewInfo>,
    pub documents: Vec<DocumentInfo>,
}

/// A document stored within an interface (realm).
#[derive(Debug, Serialize)]
pub struct DocumentInfo {
    pub name: String,
    pub data_size_bytes: usize,
    /// Number of chat messages (only populated for "chat" documents).
    pub chat_message_count: Option<usize>,
    /// Hex-encoded IDs of the last few messages (for diffing across instances).
    pub recent_message_ids: Option<Vec<String>>,
}

/// A member within an interface.
#[derive(Debug, Serialize)]
pub struct MemberViewInfo {
    pub peer_id: String,
    pub role: String,
    pub active: bool,
    pub joined_at_millis: i64,
}

/// A known peer from the peer registry.
#[derive(Debug, Serialize)]
pub struct PeerViewInfo {
    pub peer_id: String,
    pub display_name: Option<String>,
    pub first_seen_millis: i64,
    pub last_seen_millis: i64,
    pub message_count: u64,
    pub trusted: bool,
    pub connected: bool,
    pub has_pq_encapsulation_key: bool,
    pub has_pq_verifying_key: bool,
}

/// Transport-level connectivity snapshot.
#[derive(Debug, Serialize)]
pub struct TransportInfo {
    pub connected_peers: Vec<String>,
    pub discovered_peers: Vec<String>,
    pub active_realm_topics: Vec<String>,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

impl WorldView {
    /// Build a world view snapshot from the current network state.
    pub async fn build(network: &IndrasNetwork) -> Self {
        let node = network.node();
        let storage = node.storage();
        let now = chrono::Utc::now().to_rfc3339();

        // --- Node info ---
        let identity = node.identity();
        let endpoint_addr = node.endpoint_addr().await.map(|addr| format!("{:?}", addr));

        let node_info = NodeInfo {
            display_name: network.display_name().map(|s| s.to_string()),
            iroh_public_key: hex(&identity.as_bytes()),
            member_id: hex(&network.id()[..16]),
            endpoint_addr,
            data_dir: network.config().data_dir.display().to_string(),
        };

        // --- Interfaces ---
        let mut interfaces = Vec::new();
        if let Ok(records) = storage.interface_store().all() {
            for record in records {
                let iface_id = indras_core::InterfaceId::new(record.interface_id);

                // Load members for this interface
                let members = match storage.interface_store().get_members(&iface_id) {
                    Ok(members) => members
                        .into_iter()
                        .map(|m| MemberViewInfo {
                            peer_id: hex(&m.peer_id),
                            role: m.role,
                            active: m.active,
                            joined_at_millis: m.joined_at_millis,
                        })
                        .collect(),
                    Err(_) => Vec::new(),
                };

                // Load documents for this interface
                let documents = match storage.interface_store().list_documents(&iface_id) {
                    Ok(docs) => docs
                        .into_iter()
                        .map(|(name, data)| {
                            let mut info = DocumentInfo {
                                data_size_bytes: data.len(),
                                chat_message_count: None,
                                recent_message_ids: None,
                                name: name.clone(),
                            };

                            // For chat documents, try to deserialize and extract message info
                            if name == "chat" {
                                if let Ok(chat_doc) = postcard::from_bytes::<RealmChatDocument>(&data) {
                                    info.chat_message_count = Some(chat_doc.total_count());
                                    // Include last 5 message IDs for easy diffing
                                    let sorted = chat_doc.messages_sorted();
                                    let recent: Vec<String> = sorted
                                        .iter()
                                        .rev()
                                        .take(5)
                                        .map(|m| m.id.clone())
                                        .collect();
                                    info.recent_message_ids = Some(recent);
                                }
                            }

                            info
                        })
                        .collect(),
                    Err(_) => Vec::new(),
                };

                interfaces.push(InterfaceInfo {
                    id: hex(&record.interface_id),
                    name: record.name,
                    event_count: record.event_count,
                    member_count: record.member_count,
                    encrypted: record.encrypted,
                    created_at_millis: record.created_at_millis,
                    last_activity_millis: record.last_activity_millis,
                    members,
                    documents,
                });
            }
        }

        // --- Peers ---
        let connected_set: std::collections::HashSet<Vec<u8>> =
            if let Some(transport) = node.transport().await {
                transport
                    .connected_peers()
                    .into_iter()
                    .map(|p: IrohIdentity| p.as_bytes())
                    .collect()
            } else {
                std::collections::HashSet::new()
            };

        let mut peers = Vec::new();
        if let Ok(records) = storage.peer_registry().all() {
            for record in records {
                peers.push(PeerViewInfo {
                    peer_id: hex(&record.peer_id),
                    display_name: record.display_name,
                    first_seen_millis: record.first_seen_millis,
                    last_seen_millis: record.last_seen_millis,
                    message_count: record.message_count,
                    trusted: record.trusted,
                    connected: connected_set.contains(&record.peer_id),
                    has_pq_encapsulation_key: record.pq_encapsulation_key.is_some(),
                    has_pq_verifying_key: record.pq_verifying_key.is_some(),
                });
            }
        }

        // --- Transport ---
        let transport_info = if let Some(transport) = node.transport().await {
            let connected = transport
                .connected_peers()
                .into_iter()
                .map(|p: IrohIdentity| hex(&p.as_bytes()))
                .collect();

            let discovery = transport.discovery_service();
            let discovered = discovery
                .known_peers()
                .into_iter()
                .map(|p| hex(&p.identity.as_bytes()))
                .collect();
            let active_realms = discovery
                .active_realms()
                .into_iter()
                .map(|id| hex(id.as_bytes()))
                .collect();

            TransportInfo {
                connected_peers: connected,
                discovered_peers: discovered,
                active_realm_topics: active_realms,
            }
        } else {
            TransportInfo {
                connected_peers: Vec::new(),
                discovered_peers: Vec::new(),
                active_realm_topics: Vec::new(),
            }
        };

        WorldView {
            timestamp: now,
            node: node_info,
            interfaces,
            peers,
            transport: transport_info,
        }
    }

    /// Serialize and write this world view to a JSON file.
    pub fn save(&self, path: &Path) -> Result<()> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| IndraError::InvalidOperation(e.to_string()))?;
        std::fs::write(path, &json)
            .map_err(|e| IndraError::InvalidOperation(e.to_string()))?;
        Ok(())
    }
}
