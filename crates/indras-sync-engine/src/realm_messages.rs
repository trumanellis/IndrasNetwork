//! Extension trait adding message methods to Realm.

use crate::message::{MessageDocument, MessageId, StoredMessage};
use indras_network::document::Document;
use indras_network::error::Result;
use indras_network::member::MemberId;
use indras_network::Realm;
use tracing::debug;

/// Message management extension trait for Realm.
///
/// This trait provides CRDT-backed messaging that syncs across peers,
/// unlike the event-based `realm.send()` which only stores locally.
pub trait RealmMessages {
    /// Get the messages document for this realm.
    ///
    /// The document is CRDT-synchronized, so calling `refresh()` on it
    /// will pull in messages from connected peers.
    async fn chat_messages(&self) -> Result<Document<MessageDocument>>;

    /// Send a text message via the CRDT document.
    ///
    /// This stores the message in the CRDT document which syncs to peers,
    /// unlike `realm.send()` which uses the event store.
    async fn send_chat_text(
        &self,
        sender: MemberId,
        text: impl Into<String> + Send,
    ) -> Result<MessageId>;

    /// Send a system message via the CRDT document.
    async fn send_chat_system(
        &self,
        sender: MemberId,
        text: impl Into<String> + Send,
    ) -> Result<MessageId>;
}

impl RealmMessages for Realm {
    async fn chat_messages(&self) -> Result<Document<MessageDocument>> {
        self.document::<MessageDocument>("messages").await
    }

    async fn send_chat_text(
        &self,
        sender: MemberId,
        text: impl Into<String> + Send,
    ) -> Result<MessageId> {
        let text_str = text.into();
        let sender_short: String = sender.iter().take(8).map(|b| format!("{:02x}", b)).collect();
        debug!(
            sender = %sender_short,
            text_len = text_str.len(),
            "Creating chat message"
        );
        let msg = StoredMessage::text(sender, text_str);
        let msg_id = msg.id;
        let msg_id_short: String = msg_id.iter().take(8).map(|b| format!("{:02x}", b)).collect();

        let doc = self.chat_messages().await?;
        let before_count = doc.read().await.len();
        debug!(
            msg_id = %msg_id_short,
            before_count = before_count,
            "Got chat_messages document, updating..."
        );
        doc.update(|d| {
            d.add(msg);
        })
        .await?;
        let after_count = doc.read().await.len();
        debug!(
            msg_id = %msg_id_short,
            after_count = after_count,
            "Chat message added to document"
        );

        Ok(msg_id)
    }

    async fn send_chat_system(
        &self,
        sender: MemberId,
        text: impl Into<String> + Send,
    ) -> Result<MessageId> {
        let msg = StoredMessage::system(sender, text);
        let msg_id = msg.id;

        let doc = self.chat_messages().await?;
        doc.update(|d| {
            d.add(msg);
        })
        .await?;

        Ok(msg_id)
    }
}
