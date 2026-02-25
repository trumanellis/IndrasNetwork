//! Peer events and info types.

use indras_network::{GlobalEvent, MemberId, RealmId};

// Re-export from lower layers so apps don't need direct deps
pub use indras_network::contacts::ContactStatus;

/// Information about a connected peer.
#[derive(Debug, Clone, PartialEq)]
pub struct PeerInfo {
    /// The peer's member identity.
    pub member_id: MemberId,
    /// Human-readable display name.
    pub display_name: String,
    /// Unix timestamp when this peer was first seen in the current session.
    pub connected_at: i64,
    /// Sentiment toward this peer: -1 = don't recommend, 0 = neutral, 1 = recommend.
    pub sentiment: i8,
    /// Connection status: Pending (invite sent) or Confirmed (bidirectional).
    pub status: ContactStatus,
}

/// Events emitted by the [`PeeringRuntime`](crate::PeeringRuntime).
#[derive(Debug, Clone)]
pub enum PeerEvent {
    /// A new peer appeared in the contacts list.
    PeerConnected {
        /// The newly connected peer.
        peer: PeerInfo,
    },
    /// A peer disappeared from the contacts list.
    PeerDisconnected {
        /// Identity of the departed peer.
        member_id: MemberId,
    },
    /// The full peer list changed (emitted on every poll diff).
    PeersChanged {
        /// Current complete peer list.
        peers: Vec<PeerInfo>,
    },
    /// A new DM conversation was opened via `connect` / `connect_by_code`.
    ConversationOpened {
        /// The DM realm created for this conversation.
        realm_id: RealmId,
        /// The remote peer in the conversation.
        peer: PeerInfo,
    },
    /// World view was saved to disk.
    WorldViewSaved,
    /// A raw network event forwarded from `IndrasNetwork::events()`.
    NetworkEvent(GlobalEvent),
    /// A contact was blocked (removed + left all shared realms).
    PeerBlocked {
        /// Identity of the blocked peer.
        member_id: MemberId,
        /// Realms that were left as part of the block cascade.
        left_realms: Vec<RealmId>,
    },
    /// Sentiment toward a peer was updated.
    SentimentChanged {
        /// Identity of the peer whose sentiment changed.
        member_id: MemberId,
        /// New sentiment value (-1, 0, or 1).
        sentiment: i8,
    },
    /// Non-fatal warning.
    Warning(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_member_id(byte: u8) -> MemberId {
        MemberId::from([byte; 32])
    }

    #[test]
    fn peer_info_partial_eq() {
        let base = PeerInfo {
            member_id: make_member_id(0x01),
            display_name: "Alice".into(),
            connected_at: 100,
            sentiment: 0,
            status: ContactStatus::Pending,
        };

        // Equal
        assert_eq!(base, base.clone());

        // Different sentiment
        let diff_sentiment = PeerInfo { sentiment: 1, ..base.clone() };
        assert_ne!(base, diff_sentiment);

        // Different status
        let diff_status = PeerInfo { status: ContactStatus::Confirmed, ..base.clone() };
        assert_ne!(base, diff_status);

        // Different member_id
        let diff_id = PeerInfo { member_id: make_member_id(0x02), ..base.clone() };
        assert_ne!(base, diff_id);
    }

    /// Test that subscribe_with_snapshot semantics work:
    /// subscriber created before data update sees the update;
    /// snapshot returns data that existed before subscribe.
    #[test]
    fn subscribe_with_snapshot_semantics() {
        use tokio::sync::{broadcast, watch};

        let (peers_tx, peers_rx) = watch::channel::<Vec<PeerInfo>>(Vec::new());
        let (event_tx, _) = broadcast::channel::<PeerEvent>(16);

        // Pre-populate some peers
        let alice = PeerInfo {
            member_id: make_member_id(0x01),
            display_name: "Alice".into(),
            connected_at: 100,
            sentiment: 1,
            status: ContactStatus::Confirmed,
        };
        peers_tx.send(vec![alice.clone()]).unwrap();

        // Simulate subscribe_with_snapshot: subscribe then snapshot
        let rx = event_tx.subscribe();
        let snapshot = peers_rx.borrow().clone();

        // Snapshot should contain Alice
        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot[0], alice);

        // Events sent AFTER subscribe should be received
        event_tx.send(PeerEvent::WorldViewSaved).unwrap();
        let mut rx = rx;
        assert!(matches!(rx.try_recv(), Ok(PeerEvent::WorldViewSaved)));
    }

    /// Verify sentiment clamping logic matches runtime behavior.
    #[test]
    fn sentiment_clamping() {
        // The runtime clamps before persisting. Test the clamp logic.
        assert_eq!(100i8.clamp(-1, 1), 1);
        assert_eq!((-100i8).clamp(-1, 1), -1);
        assert_eq!(0i8.clamp(-1, 1), 0);
        assert_eq!(1i8.clamp(-1, 1), 1);
        assert_eq!((-1i8).clamp(-1, 1), -1);
    }
}
