//! Peer events and info types.

use indras_network::{GlobalEvent, MemberId, RealmId};

/// Information about a connected peer.
#[derive(Debug, Clone)]
pub struct PeerInfo {
    /// The peer's member identity.
    pub member_id: MemberId,
    /// Human-readable display name.
    pub display_name: String,
    /// Unix timestamp when this peer was first seen in the current session.
    pub connected_at: i64,
}

/// Events emitted by the [`PeeringRuntime`](crate::PeeringRuntime).
#[derive(Debug, Clone)]
pub enum PeerEvent {
    /// A new peer appeared in the contacts list.
    PeerConnected { peer: PeerInfo },
    /// A peer disappeared from the contacts list.
    PeerDisconnected { member_id: MemberId },
    /// The full peer list changed (emitted on every poll diff).
    PeersChanged { peers: Vec<PeerInfo> },
    /// A new DM conversation was opened via `connect` / `connect_by_code`.
    ConversationOpened { realm_id: RealmId, peer: PeerInfo },
    /// World view was saved to disk.
    WorldViewSaved,
    /// A raw network event forwarded from `IndrasNetwork::events()`.
    NetworkEvent(GlobalEvent),
    /// Non-fatal warning.
    Warning(String),
}
