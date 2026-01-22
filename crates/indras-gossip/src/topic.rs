//! Per-topic handle for sending and receiving messages

use std::sync::Arc;

use indras_core::{InterfaceEvent, InterfaceId, PeerIdentity, TopicId};
use iroh::SecretKey;
use iroh_gossip::api::{GossipReceiver, GossipSender};
use n0_future::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex as TokioMutex;

use crate::error::{GossipError, GossipResult};
use crate::events::GossipNodeEvent;
use crate::message::SignedMessage;

/// Handle for interacting with a single gossip topic (interface)
#[derive(Clone)]
pub struct TopicHandle<I: PeerIdentity> {
    /// Interface ID this topic belongs to
    interface_id: InterfaceId,
    /// Topic ID derived from interface ID
    topic_id: TopicId,
    /// Sender half of the gossip topic
    sender: Arc<TokioMutex<GossipSender>>,
    /// Secret key for signing messages
    secret_key: SecretKey,
    /// Marker for identity type
    _phantom: std::marker::PhantomData<I>,
}

impl<I: PeerIdentity + Serialize> TopicHandle<I> {
    /// Create a new topic handle
    pub(crate) fn new(
        interface_id: InterfaceId,
        sender: GossipSender,
        secret_key: SecretKey,
    ) -> Self {
        Self {
            interface_id,
            topic_id: TopicId::from_interface(interface_id),
            sender: Arc::new(TokioMutex::new(sender)),
            secret_key,
            _phantom: std::marker::PhantomData,
        }
    }

    /// Get the interface ID
    pub fn interface_id(&self) -> InterfaceId {
        self.interface_id
    }

    /// Get the topic ID
    pub fn topic_id(&self) -> TopicId {
        self.topic_id
    }

    /// Broadcast an event to all topic subscribers
    pub async fn broadcast(&self, event: &InterfaceEvent<I>) -> GossipResult<()> {
        let signed = SignedMessage::sign_and_encode(&self.secret_key, event)?;

        self.sender
            .lock()
            .await
            .broadcast(signed.into())
            .await
            .map_err(|e| GossipError::BroadcastFailed(e.to_string()))
    }

    /// Broadcast raw bytes (for pre-signed messages)
    pub async fn broadcast_raw(&self, data: Vec<u8>) -> GossipResult<()> {
        self.sender
            .lock()
            .await
            .broadcast(data.into())
            .await
            .map_err(|e| GossipError::BroadcastFailed(e.to_string()))
    }
}

/// Receiver for events from a gossip topic
pub struct TopicReceiver<I: PeerIdentity> {
    /// Receiver half of the gossip topic
    receiver: GossipReceiver,
    /// Track if we've joined the mesh
    was_joined: bool,
    /// Marker for identity type
    _phantom: std::marker::PhantomData<I>,
}

impl<I: PeerIdentity + for<'de> Deserialize<'de>> TopicReceiver<I> {
    /// Create a new topic receiver
    pub(crate) fn new(receiver: GossipReceiver) -> Self {
        Self {
            receiver,
            was_joined: false,
            _phantom: std::marker::PhantomData,
        }
    }

    /// Receive the next event from the topic
    ///
    /// Returns `None` when the topic is closed.
    pub async fn recv(&mut self) -> Option<GossipResult<GossipNodeEvent<I>>> {
        loop {
            let was_joined = self.was_joined;
            let is_joined = self.receiver.is_joined();

            // Update our joined state
            if !was_joined && is_joined {
                self.was_joined = true;
            }

            match self.receiver.try_next().await {
                Ok(Some(event)) => {
                    match GossipNodeEvent::from_gossip_event(event, was_joined, is_joined) {
                        Ok(converted) => return Some(Ok(converted)),
                        Err(e) => {
                            // Log and skip invalid messages
                            tracing::warn!("Failed to process gossip event: {}", e);
                            continue;
                        }
                    }
                }
                Ok(None) => return None,
                Err(e) => return Some(Err(GossipError::Other(e.to_string()))),
            }
        }
    }

    /// Check if we've joined the gossip mesh
    pub fn is_joined(&self) -> bool {
        self.receiver.is_joined()
    }
}

/// A split topic with separate sender and receiver
pub struct SplitTopic<I: PeerIdentity> {
    /// Handle for sending messages
    pub sender: TopicHandle<I>,
    /// Receiver for incoming messages
    pub receiver: TopicReceiver<I>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_topic_id_derivation() {
        let interface_id = InterfaceId::new([0x42; 32]);
        let topic_id = TopicId::from_interface(interface_id);

        // Topic ID should be deterministic
        let topic_id2 = TopicId::from_interface(interface_id);
        assert_eq!(topic_id.0, topic_id2.0);
    }
}
