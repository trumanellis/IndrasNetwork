//! Event types and conversion from iroh-gossip events

use indras_core::PeerIdentity;
use iroh::EndpointId;
use serde::{Deserialize, Serialize};

use crate::error::GossipResult;
use crate::message::{ReceivedMessage, SignedMessage};

/// Events received from the gossip network
#[derive(Debug, Clone)]
pub enum GossipNodeEvent<I: PeerIdentity> {
    /// A new neighbor joined the gossip mesh
    NeighborUp(EndpointId),

    /// A neighbor left the gossip mesh
    NeighborDown(EndpointId),

    /// An interface event was received from a peer
    EventReceived(ReceivedMessage<I>),

    /// We fell behind and missed some messages
    Lagged,

    /// Successfully joined the gossip mesh
    Joined {
        /// List of current neighbors
        neighbors: Vec<EndpointId>,
    },
}

impl<I: PeerIdentity + for<'de> Deserialize<'de>> GossipNodeEvent<I> {
    /// Convert from iroh-gossip event
    pub fn from_gossip_event(
        event: iroh_gossip::api::Event,
        was_joined: bool,
        is_joined: bool,
    ) -> GossipResult<Self> {
        use iroh_gossip::api::Event as GE;

        match event {
            GE::NeighborUp(id) => {
                // If we just joined, emit Joined event instead
                if !was_joined && is_joined {
                    Ok(GossipNodeEvent::Joined {
                        neighbors: vec![id],
                    })
                } else {
                    Ok(GossipNodeEvent::NeighborUp(id))
                }
            }
            GE::NeighborDown(id) => Ok(GossipNodeEvent::NeighborDown(id)),
            GE::Received(msg) => {
                let received: ReceivedMessage<I> = SignedMessage::verify_and_decode(&msg.content)?;
                Ok(GossipNodeEvent::EventReceived(received))
            }
            GE::Lagged => Ok(GossipNodeEvent::Lagged),
        }
    }
}

/// Simplified event type for external consumers who don't need full details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SimpleGossipEvent {
    /// A new neighbor joined
    NeighborUp { endpoint_id: String },
    /// A neighbor left
    NeighborDown { endpoint_id: String },
    /// Message received (contains serialized event bytes)
    MessageReceived {
        from_endpoint: String,
        from_public_key: String,
        timestamp: u64,
    },
    /// Fell behind
    Lagged,
    /// Joined the mesh
    Joined { neighbor_count: usize },
}

impl<I: PeerIdentity> From<&GossipNodeEvent<I>> for SimpleGossipEvent {
    fn from(event: &GossipNodeEvent<I>) -> Self {
        match event {
            GossipNodeEvent::NeighborUp(id) => SimpleGossipEvent::NeighborUp {
                endpoint_id: id.to_string(),
            },
            GossipNodeEvent::NeighborDown(id) => SimpleGossipEvent::NeighborDown {
                endpoint_id: id.to_string(),
            },
            GossipNodeEvent::EventReceived(msg) => SimpleGossipEvent::MessageReceived {
                from_endpoint: msg.from.to_string(),
                from_public_key: msg.from.to_string(),
                timestamp: msg.timestamp,
            },
            GossipNodeEvent::Lagged => SimpleGossipEvent::Lagged,
            GossipNodeEvent::Joined { neighbors } => SimpleGossipEvent::Joined {
                neighbor_count: neighbors.len(),
            },
        }
    }
}
