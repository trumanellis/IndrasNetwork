//! Extension trait adding note methods to Realm.

use crate::note::{Note, NoteDocument, NoteId};
use indras_network::document::Document;
use indras_network::error::Result;
use indras_network::member::MemberId;
use indras_network::Realm;

/// Note management extension trait for Realm.
pub trait RealmNotes {
    /// Get the notes document for this realm.
    async fn notes(&self) -> Result<Document<NoteDocument>>;

    /// Create a new note.
    async fn create_note(
        &self,
        title: impl Into<String> + Send,
        content: impl Into<String> + Send,
        author: MemberId,
        tags: Vec<String>,
    ) -> Result<NoteId>;

    /// Update a note's content.
    async fn update_note(
        &self,
        note_id: NoteId,
        content: impl Into<String> + Send,
    ) -> Result<()>;

    /// Delete a note.
    async fn delete_note(
        &self,
        note_id: NoteId,
    ) -> Result<Option<Note>>;
}

impl RealmNotes for Realm {
    async fn notes(&self) -> Result<Document<NoteDocument>> {
        self.document::<NoteDocument>("notes").await
    }

    async fn create_note(
        &self,
        title: impl Into<String> + Send,
        content: impl Into<String> + Send,
        author: MemberId,
        tags: Vec<String>,
    ) -> Result<NoteId> {
        let note = Note::with_tags(title, content, author, tags);
        let note_id = note.id;

        let doc = self.notes().await?;
        doc.update(|d| {
            d.add(note);
        })
        .await?;

        Ok(note_id)
    }

    async fn update_note(&self, note_id: NoteId, content: impl Into<String> + Send) -> Result<()> {
        let content = content.into();
        let doc = self.notes().await?;
        doc.update(|d| {
            if let Some(note) = d.find_mut(&note_id) {
                note.update_content(content);
            }
        })
        .await?;

        Ok(())
    }

    async fn delete_note(&self, note_id: NoteId) -> Result<Option<Note>> {
        let mut removed = None;
        let doc = self.notes().await?;
        doc.update(|d| {
            removed = d.remove(&note_id);
        })
        .await?;

        Ok(removed)
    }
}
