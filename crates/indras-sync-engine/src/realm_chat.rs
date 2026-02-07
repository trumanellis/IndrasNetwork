//! Extension trait adding editable chat methods to Realm.
//!
//! Uses `RealmChatDocument` from indras-network for rich messages with
//! edit/delete support and version history. Uses document name `"chat"`
//! to avoid collision with the older `MessageDocument` (`"messages"`).

use indras_network::chat_message::{
    ChatMessageId, EditableChatMessage, EditableMessageType, RealmChatDocument,
};
use indras_network::document::Document;
use indras_network::error::Result;
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
}
