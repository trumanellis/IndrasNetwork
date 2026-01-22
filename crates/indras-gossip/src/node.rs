//! Main gossip node wrapper around iroh-gossip

use dashmap::DashMap;
use indras_core::{InterfaceId, PeerIdentity, TopicId};
use iroh::{Endpoint, EndpointId, SecretKey};
use iroh_gossip::net::{Gossip, GOSSIP_ALPN};
use serde::{Deserialize, Serialize};

use crate::error::{GossipError, GossipResult};
use crate::topic::{SplitTopic, TopicHandle, TopicReceiver};

/// Main gossip node that manages subscriptions to multiple topics
pub struct IndrasGossip<I: PeerIdentity> {
    /// The underlying iroh-gossip instance
    gossip: Gossip,
    /// Secret key for signing messages
    secret_key: SecretKey,
    /// Active topic subscriptions
    topics: DashMap<TopicId, TopicHandle<I>>,
    /// Our endpoint ID
    endpoint_id: EndpointId,
}

impl<I: PeerIdentity + Serialize + for<'de> Deserialize<'de>> IndrasGossip<I> {
    /// Create a new gossip node
    ///
    /// This spawns the gossip protocol handler on the endpoint.
    pub fn new(endpoint: &Endpoint, secret_key: SecretKey) -> Self {
        let gossip = Gossip::builder().spawn(endpoint.clone());
        let endpoint_id = endpoint.id();

        Self {
            gossip,
            secret_key,
            topics: DashMap::new(),
            endpoint_id,
        }
    }

    /// Get the underlying Gossip instance for router registration
    ///
    /// Use this with `Router::builder().accept(GOSSIP_ALPN, gossip.gossip())`
    pub fn gossip(&self) -> &Gossip {
        &self.gossip
    }

    /// Get the ALPN protocol identifier for gossip
    pub fn alpn() -> &'static [u8] {
        GOSSIP_ALPN
    }

    /// Get our endpoint ID
    pub fn endpoint_id(&self) -> EndpointId {
        self.endpoint_id
    }

    /// Subscribe to an interface's gossip topic
    ///
    /// Returns a split topic with separate sender and receiver handles.
    pub async fn subscribe(
        &self,
        interface_id: InterfaceId,
        bootstrap: Vec<EndpointId>,
    ) -> GossipResult<SplitTopic<I>> {
        let topic_id = TopicId::from_interface(interface_id);

        // Check if already subscribed
        if self.topics.contains_key(&topic_id) {
            return Err(GossipError::AlreadySubscribed);
        }

        // Subscribe to the gossip topic
        let gossip_topic = self
            .gossip
            .subscribe(topic_id.0.into(), bootstrap)
            .await
            .map_err(|e| GossipError::SubscribeFailed(e.to_string()))?;

        // Split into sender and receiver
        let (sender, receiver) = gossip_topic.split();

        // Create our handles
        let handle = TopicHandle::new(interface_id, sender, self.secret_key.clone());
        let receiver = TopicReceiver::new(receiver);

        // Store the sender handle for later access
        self.topics.insert(topic_id, handle.clone());

        Ok(SplitTopic {
            sender: handle,
            receiver,
        })
    }

    /// Get an existing topic handle (sender only)
    pub fn get_topic(&self, interface_id: InterfaceId) -> Option<TopicHandle<I>> {
        let topic_id = TopicId::from_interface(interface_id);
        self.topics.get(&topic_id).map(|r| r.clone())
    }

    /// Unsubscribe from a topic
    pub fn unsubscribe(&self, interface_id: InterfaceId) {
        let topic_id = TopicId::from_interface(interface_id);
        self.topics.remove(&topic_id);
        // Note: The actual gossip subscription will be cleaned up when the handles are dropped
    }

    /// Check if subscribed to a topic
    pub fn is_subscribed(&self, interface_id: InterfaceId) -> bool {
        let topic_id = TopicId::from_interface(interface_id);
        self.topics.contains_key(&topic_id)
    }

    /// Get list of subscribed interface IDs
    pub fn subscribed_interfaces(&self) -> Vec<InterfaceId> {
        self.topics
            .iter()
            .map(|r| r.value().interface_id())
            .collect()
    }

    /// Get number of active subscriptions
    pub fn subscription_count(&self) -> usize {
        self.topics.len()
    }
}

/// Builder for creating an IndrasGossip instance
pub struct IndrasGossipBuilder {
    secret_key: Option<SecretKey>,
}

impl IndrasGossipBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self { secret_key: None }
    }

    /// Set the secret key for signing messages
    pub fn secret_key(mut self, key: SecretKey) -> Self {
        self.secret_key = Some(key);
        self
    }

    /// Build the gossip node
    pub fn build<I: PeerIdentity + Serialize + for<'de> Deserialize<'de>>(
        self,
        endpoint: &Endpoint,
    ) -> IndrasGossip<I> {
        let secret_key = self
            .secret_key
            .unwrap_or_else(|| SecretKey::generate(&mut rand::rng()));
        IndrasGossip::new(endpoint, secret_key)
    }
}

impl Default for IndrasGossipBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builder_default() {
        let builder = IndrasGossipBuilder::new();
        assert!(builder.secret_key.is_none());
    }

    #[test]
    fn test_builder_with_key() {
        let key = SecretKey::generate(&mut rand::rng());
        let key_public = key.public();
        let builder = IndrasGossipBuilder::new().secret_key(key);
        assert!(builder.secret_key.is_some());
        assert_eq!(builder.secret_key.unwrap().public(), key_public);
    }
}
