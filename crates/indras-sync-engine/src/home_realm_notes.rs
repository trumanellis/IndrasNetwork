//! Extension trait adding note methods to HomeRealm.
//!
//! This mirrors the pattern used by `RealmNotes` for shared realms,
//! but tailored for the personal home realm where `self.member_id()`
//! is always the author.

use crate::note::{Note, NoteDocument, NoteId};
use indras_network::document::Document;
use indras_network::error::Result;
use indras_network::home_realm::HomeRealm;

/// Note management extension trait for HomeRealm.
///
/// Provides convenience methods for personal note management.
/// All notes are created with `self.member_id()` as the author,
/// since the home realm is personal.
///
/// # Example
///
/// ```ignore
/// use indras_sync_engine::prelude::*;
///
/// let home = network.home_realm().await?;
/// let note_id = home.create_note(
///     "Meeting Notes",
///     "# Project Update\n\n- Item 1\n- Item 2",
///     vec!["work".into(), "meeting".into()],
/// ).await?;
/// home.update_note(note_id, "# Updated\n\nNew content").await?;
/// ```
pub trait HomeRealmNotes {
    /// Get the notes document for this home realm.
    async fn notes(&self) -> Result<Document<NoteDocument>>;

    /// Create a new personal note.
    ///
    /// The author is automatically set to `self.member_id()`.
    async fn create_note(
        &self,
        title: impl Into<String> + Send,
        content: impl Into<String> + Send,
        tags: Vec<String>,
    ) -> Result<NoteId>;

    /// Update an existing note's content.
    async fn update_note(
        &self,
        note_id: NoteId,
        content: impl Into<String> + Send,
    ) -> Result<()>;

    /// Delete a note. Returns the removed note if found.
    async fn delete_note(
        &self,
        note_id: NoteId,
    ) -> Result<Option<Note>>;
}

impl HomeRealmNotes for HomeRealm {
    async fn notes(&self) -> Result<Document<NoteDocument>> {
        self.document::<NoteDocument>("notes").await
    }

    async fn create_note(
        &self,
        title: impl Into<String> + Send,
        content: impl Into<String> + Send,
        tags: Vec<String>,
    ) -> Result<NoteId> {
        let note = Note::with_tags(title, content, self.member_id(), tags);
        let note_id = note.id;

        let doc = self.notes().await?;
        doc.update(|d| {
            d.add(note);
        })
        .await?;

        Ok(note_id)
    }

    async fn update_note(
        &self,
        note_id: NoteId,
        content: impl Into<String> + Send,
    ) -> Result<()> {
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
