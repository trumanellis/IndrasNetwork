//! High-level messaging client API

use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use dashmap::DashMap;
use indras_core::{InterfaceEvent, InterfaceId, PeerIdentity};
use indras_crypto::InterfaceKey;
use indras_gossip::{IndrasGossip, TopicHandle};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

use crate::error::{MessagingError, MessagingResult};
use crate::history::MessageHistory;
use crate::message::{Message, MessageContent, MessageId};

/// A joined interface with its state
struct JoinedInterface<I: PeerIdentity> {
    /// Encryption key for the interface (contains interface_id)
    key: InterfaceKey,
    /// Topic handle for gossip
    topic: TopicHandle<I>,
    /// Next sequence number for messages
    next_sequence: AtomicU64,
    /// Known members
    members: HashSet<I>,
}

/// High-level messaging client
///
/// Provides a simple API for sending and receiving messages across interfaces.
pub struct MessagingClient<I: PeerIdentity + Serialize + for<'de> Deserialize<'de>> {
    /// Our identity
    identity: I,
    /// Gossip node for broadcast
    gossip: Arc<IndrasGossip<I>>,
    /// Joined interfaces
    interfaces: DashMap<InterfaceId, JoinedInterface<I>>,
    /// Message history
    history: Arc<MessageHistory<I>>,
    /// Broadcast channel for received messages
    message_tx: broadcast::Sender<Message<I>>,
}

impl<I: PeerIdentity + Serialize + for<'de> Deserialize<'de> + Clone> MessagingClient<I> {
    /// Create a new messaging client
    pub fn new(identity: I, gossip: Arc<IndrasGossip<I>>) -> Self {
        let (message_tx, _) = broadcast::channel(1024);

        Self {
            identity,
            gossip,
            interfaces: DashMap::new(),
            history: Arc::new(MessageHistory::new()),
            message_tx,
        }
    }

    /// Create with custom history configuration
    pub fn with_history(
        identity: I,
        gossip: Arc<IndrasGossip<I>>,
        history: MessageHistory<I>,
    ) -> Self {
        let (message_tx, _) = broadcast::channel(1024);

        Self {
            identity,
            gossip,
            interfaces: DashMap::new(),
            history: Arc::new(history),
            message_tx,
        }
    }

    /// Get our identity
    pub fn identity(&self) -> &I {
        &self.identity
    }

    /// Create a new interface
    ///
    /// Returns the interface ID and the shared key that can be given to others
    /// to join the interface.
    pub async fn create_interface(&self) -> MessagingResult<(InterfaceId, InterfaceKey)> {
        let interface_id = InterfaceId::generate();
        let key = InterfaceKey::generate(interface_id);

        // Subscribe to gossip topic
        let split = self.gossip.subscribe(interface_id, vec![]).await?;

        // Store interface state
        let mut members = HashSet::new();
        members.insert(self.identity.clone());

        let joined = JoinedInterface {
            key: key.clone(),
            topic: split.sender,
            next_sequence: AtomicU64::new(1),
            members,
        };

        self.interfaces.insert(interface_id, joined);

        // Spawn receiver task
        self.spawn_receiver_task(interface_id, split.receiver);

        Ok((interface_id, key))
    }

    /// Join an existing interface using a shared key
    ///
    /// The key should be obtained from an existing member of the interface.
    pub async fn join_interface(
        &self,
        key: InterfaceKey,
        bootstrap: Vec<iroh::EndpointId>,
    ) -> MessagingResult<InterfaceId> {
        let interface_id = key.interface_id();

        // Check if already joined
        if self.interfaces.contains_key(&interface_id) {
            return Err(MessagingError::AlreadyJoined);
        }

        // Subscribe to gossip topic
        let split = self.gossip.subscribe(interface_id, bootstrap).await?;

        // Store interface state
        let mut members = HashSet::new();
        members.insert(self.identity.clone());

        let joined = JoinedInterface {
            key,
            topic: split.sender,
            next_sequence: AtomicU64::new(1),
            members,
        };

        self.interfaces.insert(interface_id, joined);

        // Spawn receiver task
        self.spawn_receiver_task(interface_id, split.receiver);

        Ok(interface_id)
    }

    /// Leave an interface
    pub fn leave_interface(&self, interface_id: &InterfaceId) -> MessagingResult<()> {
        if self.interfaces.remove(interface_id).is_none() {
            return Err(MessagingError::InterfaceNotFound(interface_id.to_string()));
        }

        self.gossip.unsubscribe(*interface_id);
        Ok(())
    }

    /// Get the key for an interface (for sharing with others)
    pub fn get_interface_key(&self, interface_id: &InterfaceId) -> Option<InterfaceKey> {
        self.interfaces.get(interface_id).map(|i| i.key.clone())
    }

    /// Send a message to an interface
    pub async fn send(
        &self,
        interface_id: &InterfaceId,
        content: MessageContent,
    ) -> MessagingResult<MessageId> {
        let interface = self
            .interfaces
            .get(interface_id)
            .ok_or_else(|| MessagingError::InterfaceNotFound(interface_id.to_string()))?;

        let sequence = interface.next_sequence.fetch_add(1, Ordering::SeqCst);
        let message = Message::new(*interface_id, self.identity.clone(), sequence, content);
        let msg_id = message.id;

        // Create interface event
        let event = InterfaceEvent::message(
            self.identity.clone(),
            sequence,
            postcard::to_allocvec(&message)?,
        );

        // Broadcast via gossip
        interface.topic.broadcast(&event).await?;

        // Store in history
        self.history.store(message)?;

        Ok(msg_id)
    }

    /// Send a text message
    pub async fn send_text(
        &self,
        interface_id: &InterfaceId,
        text: impl Into<String>,
    ) -> MessagingResult<MessageId> {
        self.send(interface_id, MessageContent::text(text)).await
    }

    /// Reply to a message
    pub async fn reply(
        &self,
        interface_id: &InterfaceId,
        reply_to: MessageId,
        content: MessageContent,
    ) -> MessagingResult<MessageId> {
        let interface = self
            .interfaces
            .get(interface_id)
            .ok_or_else(|| MessagingError::InterfaceNotFound(interface_id.to_string()))?;

        let sequence = interface.next_sequence.fetch_add(1, Ordering::SeqCst);
        let message = Message::reply(
            *interface_id,
            self.identity.clone(),
            sequence,
            content,
            reply_to,
        );
        let msg_id = message.id;

        // Create interface event
        let event = InterfaceEvent::message(
            self.identity.clone(),
            sequence,
            postcard::to_allocvec(&message)?,
        );

        // Broadcast via gossip
        interface.topic.broadcast(&event).await?;

        // Store in history
        self.history.store(message)?;

        Ok(msg_id)
    }

    /// Get a subscription to received messages
    pub fn messages(&self) -> broadcast::Receiver<Message<I>> {
        self.message_tx.subscribe()
    }

    /// Get message history
    pub fn history(&self) -> &MessageHistory<I> {
        &self.history
    }

    /// Get list of joined interfaces
    pub fn interfaces(&self) -> Vec<InterfaceId> {
        self.interfaces.iter().map(|r| *r.key()).collect()
    }

    /// Check if joined to an interface
    pub fn is_joined(&self, interface_id: &InterfaceId) -> bool {
        self.interfaces.contains_key(interface_id)
    }

    /// Get members of an interface
    pub fn members(&self, interface_id: &InterfaceId) -> Option<HashSet<I>> {
        self.interfaces.get(interface_id).map(|i| i.members.clone())
    }

    /// Spawn a task to receive messages from gossip
    fn spawn_receiver_task(
        &self,
        interface_id: InterfaceId,
        mut receiver: indras_gossip::TopicReceiver<I>,
    ) {
        let history = self.history.clone();
        let message_tx = self.message_tx.clone();
        let our_identity = self.identity.clone();

        tokio::spawn(async move {
            while let Some(result) = receiver.recv().await {
                match result {
                    Ok(event) => {
                        if let indras_gossip::GossipNodeEvent::EventReceived(received) = event {
                            // Parse the message from the event
                            if let InterfaceEvent::Message { sender, content, .. } = received.event {
                                // Skip our own messages (already stored)
                                if sender == our_identity {
                                    continue;
                                }

                                // Try to decode the message
                                if let Ok(message) = postcard::from_bytes::<Message<I>>(&content) {
                                    // Store in history
                                    let _ = history.store(message.clone());

                                    // Broadcast to subscribers
                                    let _ = message_tx.send(message);
                                }
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Error receiving from gossip for {}: {}", interface_id, e);
                    }
                }
            }
            tracing::debug!("Receiver task for {} exited", interface_id);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: Full integration tests require an iroh endpoint, which is tested
    // in the integration tests. Here we test the simpler synchronous parts.

    #[test]
    fn test_message_content_creation() {
        let text = MessageContent::text("Hello");
        assert!(text.is_text());
        assert_eq!(text.as_text(), Some("Hello"));

        let binary = MessageContent::binary("application/octet-stream", vec![1, 2, 3]);
        assert!(!binary.is_text());
    }

    #[test]
    fn test_interface_id_from_key() {
        let interface_id = InterfaceId::generate();
        let key = InterfaceKey::generate(interface_id);
        let id_from_key = InterfaceId::from_key_bytes(key.as_bytes());

        // Derived ID should be deterministic
        let id_from_key2 = InterfaceId::from_key_bytes(key.as_bytes());
        assert_eq!(id_from_key, id_from_key2);
    }
}
