//! System events for inline display in the chat timeline.
//!
//! These are ephemeral, in-memory events (not persisted in CRDT) that surface
//! transport, gossip, and sync activity as human-readable system messages.

/// An ephemeral system event for inline display in the chat timeline.
#[derive(Debug, Clone)]
pub enum SystemEvent {
    // -- Transport / discovery --
    /// A new peer was discovered on the network.
    PeerDiscovered {
        peer_id_short: String,
        peer_name: Option<String>,
        timestamp: u64,
    },
    /// A peer's presence information was updated.
    PeerUpdated {
        peer_id_short: String,
        peer_name: Option<String>,
        timestamp: u64,
    },
    /// A peer went offline (timed out).
    PeerLost {
        peer_id_short: String,
        timestamp: u64,
    },

    // -- Realm gossip --
    /// A peer joined this realm via gossip.
    RealmPeerJoined {
        peer_id_short: String,
        peer_name: Option<String>,
        has_pq_keys: bool,
        timestamp: u64,
    },
    /// A peer left this realm.
    RealmPeerLeft {
        peer_id_short: String,
        timestamp: u64,
    },
    /// A peer requested introductions to other realm members.
    IntroductionRequested {
        requester_short: String,
        known_count: usize,
        timestamp: u64,
    },

    // -- Membership (CRDT interface events) --
    /// A member joined the realm (via CRDT membership change).
    MemberJoined {
        peer_id_short: String,
        timestamp: u64,
    },
    /// A member left the realm (via CRDT membership change).
    MemberLeft {
        peer_id_short: String,
        timestamp: u64,
    },
    /// The realm was created.
    RealmCreated {
        creator_short: String,
        timestamp: u64,
    },

    // -- CRDT sync --
    /// A CRDT document was synced from a remote peer.
    DocumentSynced {
        is_remote: bool,
        timestamp: u64,
    },
}

impl SystemEvent {
    /// Extract the timestamp from any variant.
    pub fn timestamp(&self) -> u64 {
        match self {
            Self::PeerDiscovered { timestamp, .. }
            | Self::PeerUpdated { timestamp, .. }
            | Self::PeerLost { timestamp, .. }
            | Self::RealmPeerJoined { timestamp, .. }
            | Self::RealmPeerLeft { timestamp, .. }
            | Self::IntroductionRequested { timestamp, .. }
            | Self::MemberJoined { timestamp, .. }
            | Self::MemberLeft { timestamp, .. }
            | Self::RealmCreated { timestamp, .. }
            | Self::DocumentSynced { timestamp, .. } => *timestamp,
        }
    }

    /// Human-readable one-liner for inline display in the chat timeline.
    pub fn display_text(&self) -> String {
        match self {
            Self::PeerDiscovered { peer_name: Some(name), .. } => {
                format!("{name} appeared on the network")
            }
            Self::PeerDiscovered { peer_id_short, .. } => {
                format!("{peer_id_short} appeared on the network")
            }
            Self::PeerUpdated { peer_name: Some(name), .. } => {
                format!("{name} updated their presence")
            }
            Self::PeerUpdated { peer_id_short, .. } => {
                format!("{peer_id_short} updated their presence")
            }
            Self::PeerLost { peer_id_short, .. } => {
                format!("{peer_id_short} went offline")
            }
            Self::RealmPeerJoined { peer_name: Some(name), has_pq_keys: true, .. } => {
                format!("{name} joined the realm (PQ-secured)")
            }
            Self::RealmPeerJoined { peer_name: Some(name), .. } => {
                format!("{name} joined the realm")
            }
            Self::RealmPeerJoined { peer_id_short, has_pq_keys: true, .. } => {
                format!("{peer_id_short} joined the realm (PQ-secured)")
            }
            Self::RealmPeerJoined { peer_id_short, .. } => {
                format!("{peer_id_short} joined the realm")
            }
            Self::RealmPeerLeft { peer_id_short, .. } => {
                format!("{peer_id_short} left the realm")
            }
            Self::IntroductionRequested { requester_short, known_count, .. } => {
                format!("{requester_short} requested introductions (knows {known_count} peers)")
            }
            Self::MemberJoined { peer_id_short, .. } => {
                format!("{peer_id_short} joined the realm")
            }
            Self::MemberLeft { peer_id_short, .. } => {
                format!("{peer_id_short} left the realm")
            }
            Self::RealmCreated { creator_short, .. } => {
                format!("{creator_short} created the realm")
            }
            Self::DocumentSynced { is_remote: true, .. } => {
                "Chat synced from peer".to_string()
            }
            Self::DocumentSynced { .. } => {
                "Chat synced locally".to_string()
            }
        }
    }
}
