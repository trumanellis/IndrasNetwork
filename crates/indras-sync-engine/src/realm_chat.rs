//! Extension trait adding editable chat methods to Realm.
//!
//! Uses `RealmChatDocument` from indras-network for rich messages with
//! edit/delete support and version history. Uses document name `"chat"`
//! to avoid collision with the older `MessageDocument` (`"messages"`).

use indras_network::chat_message::{
    ChatAck, ChatAckDocument, ChatMessageId, DeliveryStatus, EditableChatMessage,
    EditableMessageType, RealmChatDocument,
};
use indras_network::document::Document;
use indras_network::error::Result;
use indras_network::escape::PeerIdentity;
use indras_network::Realm;
use tracing::debug;

/// Editable chat extension trait for Realm.
///
/// Wraps `RealmChatDocument` behind `self.document::<RealmChatDocument>("chat")`.
/// Provides send, edit, and delete with author-only permission checks.
pub trait RealmChat {
    /// Get the editable chat document for this realm.
    async fn chat_document(&self) -> Result<Document<RealmChatDocument>>;

    /// Send a text message to the chat.
    ///
    /// Returns the message ID on success.
    async fn send_chat(
        &self,
        realm_id: &str,
        author: &str,
        content: String,
        tick: u64,
    ) -> Result<ChatMessageId>;

    /// Edit an existing message (author-only).
    ///
    /// Returns `true` if the edit was applied.
    async fn edit_chat(
        &self,
        id: &str,
        author: &str,
        new_content: String,
        tick: u64,
    ) -> Result<bool>;

    /// Delete an existing message (author-only, soft delete).
    ///
    /// Returns `true` if the delete was applied.
    async fn delete_chat(&self, id: &str, author: &str, tick: u64) -> Result<bool>;

    /// Send a reply to an existing message.
    ///
    /// Returns the new message ID on success.
    async fn send_chat_reply(
        &self,
        realm_id: &str,
        author: &str,
        content: String,
        tick: u64,
        reply_to: &str,
    ) -> Result<ChatMessageId>;

    /// Add a reaction to a message.
    ///
    /// Returns true if the reaction was added (false if duplicate).
    async fn react_chat(&self, msg_id: &str, author: &str, emoji: &str) -> Result<bool>;

    /// Remove a reaction from a message.
    ///
    /// Returns true if the reaction was removed.
    async fn unreact_chat(&self, msg_id: &str, author: &str, emoji: &str) -> Result<bool>;

    /// Get the chat-acks document for this realm.
    async fn chat_ack_document(&self) -> Result<Document<ChatAckDocument>>;

    /// Send a delivery acknowledgement for a list of message IDs.
    ///
    /// Called when remote chat messages are received and merged.
    async fn send_chat_acks(&self, message_ids: &[String], tick: u64) -> Result<()>;

    /// Query the delivery status of a message.
    async fn chat_delivery_status(&self, message_id: &str) -> Result<DeliveryStatus>;

    /// Spawn a background task that automatically ACKs incoming remote chat
    /// messages. Returns a `JoinHandle` the caller can use to cancel.
    async fn spawn_chat_ack_responder(&self, tick: u64) -> Result<tokio::task::JoinHandle<()>>;
}

impl RealmChat for Realm {
    async fn chat_document(&self) -> Result<Document<RealmChatDocument>> {
        self.document::<RealmChatDocument>("chat").await
    }

    async fn send_chat(
        &self,
        realm_id: &str,
        author: &str,
        content: String,
        tick: u64,
    ) -> Result<ChatMessageId> {
        let msg_id = format!("{}-{}-{}", realm_id, tick, &author[..8.min(author.len())]);
        debug!(
            msg_id = %msg_id,
            author = %&author[..16.min(author.len())],
            content_len = content.len(),
            "Sending editable chat message"
        );

        let msg = EditableChatMessage::new(
            msg_id.clone(),
            realm_id.to_string(),
            author.to_string(),
            content,
            tick,
            EditableMessageType::Text,
        );

        let doc = self.chat_document().await?;
        doc.update(|d| {
            d.add_message(msg);
        })
        .await?;

        debug!(msg_id = %msg_id, "Editable chat message sent");
        Ok(msg_id)
    }

    async fn edit_chat(
        &self,
        id: &str,
        author: &str,
        new_content: String,
        tick: u64,
    ) -> Result<bool> {
        debug!(
            msg_id = %id,
            author = %&author[..16.min(author.len())],
            "Editing chat message"
        );

        let doc = self.chat_document().await?;
        let result = doc
            .transaction(|d| d.edit_message(id, author, new_content, tick))
            .await?;

        debug!(msg_id = %id, edited = result, "Edit chat result");
        Ok(result)
    }

    async fn delete_chat(&self, id: &str, author: &str, tick: u64) -> Result<bool> {
        debug!(
            msg_id = %id,
            author = %&author[..16.min(author.len())],
            "Deleting chat message"
        );

        let doc = self.chat_document().await?;
        let result = doc
            .transaction(|d| d.delete_message(id, author, tick))
            .await?;

        debug!(msg_id = %id, deleted = result, "Delete chat result");
        Ok(result)
    }

    async fn send_chat_reply(
        &self,
        realm_id: &str,
        author: &str,
        content: String,
        tick: u64,
        reply_to: &str,
    ) -> Result<ChatMessageId> {
        let msg_id = format!("{}-{}-{}", realm_id, tick, &author[..8.min(author.len())]);
        debug!(
            msg_id = %msg_id,
            author = %&author[..16.min(author.len())],
            reply_to = %reply_to,
            "Sending chat reply"
        );

        let msg = EditableChatMessage::new_reply(
            msg_id.clone(),
            realm_id.to_string(),
            author.to_string(),
            content,
            tick,
            EditableMessageType::Text,
            reply_to.to_string(),
        );

        let doc = self.chat_document().await?;
        doc.update(|d| {
            d.add_message(msg);
        })
        .await?;

        debug!(msg_id = %msg_id, "Chat reply sent");
        Ok(msg_id)
    }

    async fn react_chat(&self, msg_id: &str, author: &str, emoji: &str) -> Result<bool> {
        debug!(
            msg_id = %msg_id,
            author = %&author[..16.min(author.len())],
            emoji = %emoji,
            "Adding reaction"
        );

        let doc = self.chat_document().await?;
        let result = doc
            .transaction(|d| d.add_reaction(msg_id, author, emoji))
            .await?;

        debug!(msg_id = %msg_id, added = result, "React result");
        Ok(result)
    }

    async fn unreact_chat(&self, msg_id: &str, author: &str, emoji: &str) -> Result<bool> {
        debug!(
            msg_id = %msg_id,
            author = %&author[..16.min(author.len())],
            emoji = %emoji,
            "Removing reaction"
        );

        let doc = self.chat_document().await?;
        let result = doc
            .transaction(|d| d.remove_reaction(msg_id, author, emoji))
            .await?;

        debug!(msg_id = %msg_id, removed = result, "Unreact result");
        Ok(result)
    }

    async fn chat_ack_document(&self) -> Result<Document<ChatAckDocument>> {
        self.document::<ChatAckDocument>("chat-acks").await
    }

    async fn send_chat_acks(&self, message_ids: &[String], tick: u64) -> Result<()> {
        if message_ids.is_empty() {
            return Ok(());
        }

        let our_id = hex::encode(self.node().identity().as_bytes());
        let ack_doc = self.chat_ack_document().await?;

        ack_doc
            .update(|d| {
                for msg_id in message_ids {
                    let ack = ChatAck {
                        message_id: msg_id.clone(),
                        receiver: our_id.clone(),
                        acked_at: tick,
                    };
                    d.record_ack(&ack);
                }
            })
            .await?;

        debug!(
            count = message_ids.len(),
            "Sent chat delivery ACKs"
        );
        Ok(())
    }

    async fn chat_delivery_status(&self, message_id: &str) -> Result<DeliveryStatus> {
        let ack_doc = self.chat_ack_document().await?;
        let state = ack_doc.read().await;
        Ok(state.delivery_status(message_id))
    }

    async fn spawn_chat_ack_responder(&self, tick: u64) -> Result<tokio::task::JoinHandle<()>> {
        let chat_doc = self.chat_document().await?;
        let mut rx = chat_doc.subscribe();
        let realm = self.clone();

        let handle = tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(change) => {
                        if !change.is_remote {
                            continue;
                        }
                        // Collect message IDs from the new state that we should ACK.
                        // We ACK all messages from the incoming change's state that
                        // weren't authored by us.
                        let our_id = hex::encode(realm.node().identity().as_bytes());
                        let msg_ids: Vec<String> = change
                            .new_state
                            .iter_messages()
                            .filter(|m| {
                                m.author_id.as_deref() != Some(&our_id)
                                    && m.author != our_id
                            })
                            .map(|m| m.id.clone())
                            .collect();

                        if !msg_ids.is_empty() {
                            if let Err(e) = realm.send_chat_acks(&msg_ids, tick).await {
                                debug!(error = %e, "Failed to send chat ACKs");
                            }
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });

        Ok(handle)
    }
}
