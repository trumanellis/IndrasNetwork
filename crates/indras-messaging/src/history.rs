//! Message history storage and querying

use std::collections::BTreeMap;
use std::sync::RwLock;

use chrono::{DateTime, Utc};
use indras_core::{InterfaceId, PeerIdentity};
use serde::{Deserialize, Serialize};

use crate::error::{MessagingError, MessagingResult};
use crate::message::{Message, MessageId};

/// Filter criteria for querying message history
#[derive(Debug, Clone)]
pub struct MessageFilter<I: PeerIdentity> {
    /// Filter by interface
    pub interface_id: Option<InterfaceId>,
    /// Filter by sender
    pub sender: Option<I>,
    /// Filter messages since this time
    pub since: Option<DateTime<Utc>>,
    /// Filter messages until this time
    pub until: Option<DateTime<Utc>>,
    /// Only include text messages
    pub text_only: bool,
    /// Maximum number of messages to return
    pub limit: Option<usize>,
    /// Offset for pagination
    pub offset: usize,
}

impl<I: PeerIdentity> Default for MessageFilter<I> {
    fn default() -> Self {
        Self {
            interface_id: None,
            sender: None,
            since: None,
            until: None,
            text_only: false,
            limit: None,
            offset: 0,
        }
    }
}

impl<I: PeerIdentity> MessageFilter<I> {
    /// Create a new filter
    pub fn new() -> Self {
        Self::default()
    }

    /// Filter by interface
    pub fn interface(mut self, id: InterfaceId) -> Self {
        self.interface_id = Some(id);
        self
    }

    /// Filter by sender
    pub fn sender(mut self, sender: I) -> Self {
        self.sender = Some(sender);
        self
    }

    /// Filter messages since a time
    pub fn since(mut self, time: DateTime<Utc>) -> Self {
        self.since = Some(time);
        self
    }

    /// Filter messages until a time
    pub fn until(mut self, time: DateTime<Utc>) -> Self {
        self.until = Some(time);
        self
    }

    /// Only include text messages
    pub fn text_only(mut self) -> Self {
        self.text_only = true;
        self
    }

    /// Limit the number of results
    pub fn limit(mut self, n: usize) -> Self {
        self.limit = Some(n);
        self
    }

    /// Set pagination offset
    pub fn offset(mut self, n: usize) -> Self {
        self.offset = n;
        self
    }
}

/// In-memory message history storage
///
/// This is a simple implementation suitable for testing and short-lived sessions.
/// For persistent storage, use a database-backed implementation.
pub struct MessageHistory<I: PeerIdentity> {
    /// Messages indexed by interface and sequence
    messages: RwLock<BTreeMap<InterfaceId, BTreeMap<u64, Message<I>>>>,
    /// Message ID to (interface_id, sequence) mapping for quick lookup
    id_index: RwLock<BTreeMap<MessageId, (InterfaceId, u64)>>,
    /// Maximum messages per interface (0 = unlimited)
    max_per_interface: usize,
}

impl<I: PeerIdentity + Clone + Serialize + for<'de> Deserialize<'de>> MessageHistory<I> {
    /// Create a new message history
    pub fn new() -> Self {
        Self {
            messages: RwLock::new(BTreeMap::new()),
            id_index: RwLock::new(BTreeMap::new()),
            max_per_interface: 0,
        }
    }

    /// Create with a maximum messages per interface limit
    pub fn with_limit(max_per_interface: usize) -> Self {
        Self {
            messages: RwLock::new(BTreeMap::new()),
            id_index: RwLock::new(BTreeMap::new()),
            max_per_interface,
        }
    }

    /// Store a message
    pub fn store(&self, message: Message<I>) -> MessagingResult<()> {
        let interface_id = message.interface_id;
        let sequence = message.id.sequence;
        let msg_id = message.id;

        let mut messages = self.messages.write().map_err(|_| {
            MessagingError::StorageError("failed to acquire write lock".to_string())
        })?;

        let interface_messages = messages.entry(interface_id).or_insert_with(BTreeMap::new);

        // Apply limit if configured
        if self.max_per_interface > 0 && interface_messages.len() >= self.max_per_interface {
            // Remove oldest message
            if let Some(oldest_seq) = interface_messages.keys().next().copied()
                && let Some(oldest_msg) = interface_messages.remove(&oldest_seq) {
                    // Also remove from ID index
                    let mut id_index = self.id_index.write().map_err(|_| {
                        MessagingError::StorageError("failed to acquire write lock".to_string())
                    })?;
                    id_index.remove(&oldest_msg.id);
                }
        }

        interface_messages.insert(sequence, message);

        // Update ID index
        let mut id_index = self.id_index.write().map_err(|_| {
            MessagingError::StorageError("failed to acquire write lock".to_string())
        })?;
        id_index.insert(msg_id, (interface_id, sequence));

        Ok(())
    }

    /// Get a message by ID
    pub fn get(&self, id: &MessageId) -> MessagingResult<Option<Message<I>>> {
        let id_index = self.id_index.read().map_err(|_| {
            MessagingError::StorageError("failed to acquire read lock".to_string())
        })?;

        let Some((interface_id, sequence)) = id_index.get(id) else {
            return Ok(None);
        };

        let messages = self.messages.read().map_err(|_| {
            MessagingError::StorageError("failed to acquire read lock".to_string())
        })?;

        Ok(messages
            .get(interface_id)
            .and_then(|m| m.get(sequence))
            .cloned())
    }

    /// Query messages with a filter
    pub fn query(&self, filter: &MessageFilter<I>) -> MessagingResult<Vec<Message<I>>> {
        let messages = self.messages.read().map_err(|_| {
            MessagingError::StorageError("failed to acquire read lock".to_string())
        })?;

        let mut results = Vec::new();

        // Determine which interfaces to search
        let interfaces: Vec<_> = if let Some(id) = filter.interface_id {
            vec![id]
        } else {
            messages.keys().cloned().collect()
        };

        for interface_id in interfaces {
            if let Some(interface_messages) = messages.get(&interface_id) {
                for message in interface_messages.values() {
                    // Apply filters
                    if let Some(ref sender) = filter.sender
                        && &message.sender != sender {
                            continue;
                        }

                    if let Some(since) = filter.since
                        && message.timestamp < since {
                            continue;
                        }

                    if let Some(until) = filter.until
                        && message.timestamp > until {
                            continue;
                        }

                    if filter.text_only && !message.content.is_text() {
                        continue;
                    }

                    results.push(message.clone());
                }
            }
        }

        // Sort by timestamp
        results.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));

        // Apply offset and limit
        let results: Vec<_> = results.into_iter().skip(filter.offset).collect();

        let results = if let Some(limit) = filter.limit {
            results.into_iter().take(limit).collect()
        } else {
            results
        };

        Ok(results)
    }

    /// Get messages from an interface since a sequence number
    pub fn since(&self, interface_id: InterfaceId, since_sequence: u64) -> MessagingResult<Vec<Message<I>>> {
        let messages = self.messages.read().map_err(|_| {
            MessagingError::StorageError("failed to acquire read lock".to_string())
        })?;

        Ok(messages
            .get(&interface_id)
            .map(|m| {
                m.range(since_sequence..)
                    .map(|(_, msg)| msg.clone())
                    .collect()
            })
            .unwrap_or_default())
    }

    /// Get the latest N messages from an interface
    pub fn latest(&self, interface_id: InterfaceId, count: usize) -> MessagingResult<Vec<Message<I>>> {
        let messages = self.messages.read().map_err(|_| {
            MessagingError::StorageError("failed to acquire read lock".to_string())
        })?;

        Ok(messages
            .get(&interface_id)
            .map(|m| {
                m.values()
                    .rev()
                    .take(count)
                    .cloned()
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect()
            })
            .unwrap_or_default())
    }

    /// Get message count for an interface
    pub fn count(&self, interface_id: InterfaceId) -> usize {
        self.messages
            .read()
            .ok()
            .and_then(|m| m.get(&interface_id).map(|im| im.len()))
            .unwrap_or(0)
    }

    /// Get total message count across all interfaces
    pub fn total_count(&self) -> usize {
        self.messages
            .read()
            .ok()
            .map(|m| m.values().map(|im| im.len()).sum())
            .unwrap_or(0)
    }

    /// Clear all messages for an interface
    pub fn clear(&self, interface_id: InterfaceId) -> MessagingResult<()> {
        let mut messages = self.messages.write().map_err(|_| {
            MessagingError::StorageError("failed to acquire write lock".to_string())
        })?;

        if let Some(interface_messages) = messages.remove(&interface_id) {
            let mut id_index = self.id_index.write().map_err(|_| {
                MessagingError::StorageError("failed to acquire write lock".to_string())
            })?;

            for msg in interface_messages.values() {
                id_index.remove(&msg.id);
            }
        }

        Ok(())
    }
}

impl<I: PeerIdentity + Clone + Serialize + for<'de> Deserialize<'de>> Default for MessageHistory<I> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indras_core::SimulationIdentity;

    fn create_test_message(interface_id: InterfaceId, sender: char, seq: u64, text: &str) -> Message<SimulationIdentity> {
        Message::text(
            interface_id,
            SimulationIdentity::new(sender).unwrap(),
            seq,
            text,
        )
    }

    #[test]
    fn test_store_and_get() {
        let history: MessageHistory<SimulationIdentity> = MessageHistory::new();
        let interface_id = InterfaceId::new([0x42; 32]);
        let msg = create_test_message(interface_id, 'A', 1, "Hello");
        let msg_id = msg.id;

        history.store(msg.clone()).unwrap();

        let retrieved = history.get(&msg_id).unwrap().unwrap();
        assert_eq!(retrieved.content.as_text(), Some("Hello"));
    }

    #[test]
    fn test_query_by_interface() {
        let history: MessageHistory<SimulationIdentity> = MessageHistory::new();
        let interface1 = InterfaceId::new([0x01; 32]);
        let interface2 = InterfaceId::new([0x02; 32]);

        history.store(create_test_message(interface1, 'A', 1, "Hello 1")).unwrap();
        history.store(create_test_message(interface2, 'A', 1, "Hello 2")).unwrap();
        history.store(create_test_message(interface1, 'A', 2, "Hello 3")).unwrap();

        let filter = MessageFilter::new().interface(interface1);
        let results = history.query(&filter).unwrap();

        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_query_by_sender() {
        let history: MessageHistory<SimulationIdentity> = MessageHistory::new();
        let interface_id = InterfaceId::new([0x42; 32]);

        history.store(create_test_message(interface_id, 'A', 1, "From A")).unwrap();
        history.store(create_test_message(interface_id, 'B', 2, "From B")).unwrap();
        history.store(create_test_message(interface_id, 'A', 3, "From A again")).unwrap();

        let filter = MessageFilter::new().sender(SimulationIdentity::new('A').unwrap());
        let results = history.query(&filter).unwrap();

        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_query_limit_and_offset() {
        let history: MessageHistory<SimulationIdentity> = MessageHistory::new();
        let interface_id = InterfaceId::new([0x42; 32]);

        for i in 1..=10 {
            history.store(create_test_message(interface_id, 'A', i, &format!("Msg {}", i))).unwrap();
        }

        let filter = MessageFilter::new().limit(3).offset(2);
        let results = history.query(&filter).unwrap();

        assert_eq!(results.len(), 3);
        assert_eq!(results[0].content.as_text(), Some("Msg 3"));
    }

    #[test]
    fn test_latest() {
        let history: MessageHistory<SimulationIdentity> = MessageHistory::new();
        let interface_id = InterfaceId::new([0x42; 32]);

        for i in 1..=10 {
            history.store(create_test_message(interface_id, 'A', i, &format!("Msg {}", i))).unwrap();
        }

        let latest = history.latest(interface_id, 3).unwrap();
        assert_eq!(latest.len(), 3);
        assert_eq!(latest[0].content.as_text(), Some("Msg 8"));
        assert_eq!(latest[2].content.as_text(), Some("Msg 10"));
    }

    #[test]
    fn test_since() {
        let history: MessageHistory<SimulationIdentity> = MessageHistory::new();
        let interface_id = InterfaceId::new([0x42; 32]);

        for i in 1..=5 {
            history.store(create_test_message(interface_id, 'A', i, &format!("Msg {}", i))).unwrap();
        }

        let since = history.since(interface_id, 3).unwrap();
        assert_eq!(since.len(), 3);
        assert_eq!(since[0].content.as_text(), Some("Msg 3"));
    }

    #[test]
    fn test_limit_enforcement() {
        let history: MessageHistory<SimulationIdentity> = MessageHistory::with_limit(3);
        let interface_id = InterfaceId::new([0x42; 32]);

        for i in 1..=5 {
            history.store(create_test_message(interface_id, 'A', i, &format!("Msg {}", i))).unwrap();
        }

        assert_eq!(history.count(interface_id), 3);

        // Should have kept the latest 3 messages
        let all = history.query(&MessageFilter::new().interface(interface_id)).unwrap();
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].content.as_text(), Some("Msg 3"));
    }

    #[test]
    fn test_clear() {
        let history: MessageHistory<SimulationIdentity> = MessageHistory::new();
        let interface_id = InterfaceId::new([0x42; 32]);

        history.store(create_test_message(interface_id, 'A', 1, "Hello")).unwrap();
        assert_eq!(history.count(interface_id), 1);

        history.clear(interface_id).unwrap();
        assert_eq!(history.count(interface_id), 0);
    }
}
