//! Lua bindings for Note types
//!
//! Provides UserData implementations for Note and NoteId.

use mlua::{Lua, Result, Table, UserData, UserDataMethods};

use crate::note::Note;

/// Lua wrapper for Note (read-only)
#[derive(Clone)]
pub struct LuaNote {
    inner: Note,
}

impl LuaNote {
    pub fn new(note: Note) -> Self {
        Self { inner: note }
    }

    pub fn into_inner(self) -> Note {
        self.inner
    }

    pub fn inner(&self) -> &Note {
        &self.inner
    }
}

impl UserData for LuaNote {
    fn add_fields<F: mlua::UserDataFields<Self>>(fields: &mut F) {
        // Read-only fields
        fields.add_field_method_get("id", |_, this| Ok(this.inner.id.clone()));
        fields.add_field_method_get("title", |_, this| Ok(this.inner.title.clone()));
        fields.add_field_method_get("content", |_, this| Ok(this.inner.content.clone()));
        fields.add_field_method_get("author", |_, this| Ok(this.inner.author.clone()));
        fields.add_field_method_get("created_at", |_, this| {
            Ok(this.inner.created_at.to_rfc3339())
        });
        fields.add_field_method_get("modified_at", |_, this| {
            Ok(this.inner.modified_at.to_rfc3339())
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // preview(max_len) -> string
        methods.add_method("preview", |_, this, max_len: Option<usize>| {
            let max_len = max_len.unwrap_or(50);
            Ok(this.inner.preview(max_len))
        });

        // tostring metamethod
        methods.add_meta_method(mlua::MetaMethod::ToString, |_, this, ()| {
            Ok(format!(
                "Note({}: {})",
                &this.inner.id[..8.min(this.inner.id.len())],
                this.inner.title
            ))
        });
    }
}

/// Lua wrapper for NoteId (just a string, but typed for clarity)
#[derive(Clone)]
pub struct LuaNoteId {
    id: String,
}

impl LuaNoteId {
    pub fn new(id: String) -> Self {
        Self { id }
    }

    pub fn id(&self) -> &str {
        &self.id
    }
}

impl UserData for LuaNoteId {
    fn add_fields<F: mlua::UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("value", |_, this| Ok(this.id.clone()));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // short() -> first 8 chars
        methods.add_method("short", |_, this, ()| {
            let len = 8.min(this.id.len());
            Ok(this.id[..len].to_string())
        });

        // tostring metamethod
        methods.add_meta_method(mlua::MetaMethod::ToString, |_, this, ()| {
            Ok(this.id.clone())
        });
    }
}

/// Register note types with the notes table
pub fn register(lua: &Lua, notes: &Table) -> Result<()> {
    // Note constructor (for creating notes from Lua)
    let note_table = lua.create_table()?;

    // Note.new(title, author) -> Note
    note_table.set(
        "new",
        lua.create_function(|_, (title, author): (String, String)| {
            let note = Note::new(title, author);
            Ok(LuaNote::new(note))
        })?,
    )?;

    notes.set("Note", note_table)?;

    // NoteId constructor
    let note_id_table = lua.create_table()?;

    // NoteId.new(id) -> NoteId
    note_id_table.set(
        "new",
        lua.create_function(|_, id: String| Ok(LuaNoteId::new(id)))?,
    )?;

    notes.set("NoteId", note_id_table)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_lua() -> Lua {
        let lua = Lua::new();
        let notes = lua.create_table().unwrap();
        register(&lua, &notes).unwrap();
        lua.globals().set("notes", notes).unwrap();
        lua
    }

    #[test]
    fn test_note_creation() {
        let lua = setup_lua();
        let id: String = lua
            .load(
                r#"
            local note = notes.Note.new("Test Title", "alice")
            return note.id
        "#,
            )
            .eval()
            .unwrap();
        assert!(!id.is_empty());
    }

    #[test]
    fn test_note_fields() {
        let lua = setup_lua();
        let result: (String, String) = lua
            .load(
                r#"
            local note = notes.Note.new("Test Title", "alice")
            return note.title, note.author
        "#,
            )
            .eval()
            .unwrap();
        assert_eq!(result.0, "Test Title");
        assert_eq!(result.1, "alice");
    }

    #[test]
    fn test_note_preview() {
        let lua = setup_lua();

        // First create a note with content
        lua.scope(|_scope| {
            let note = Note::new("Test", "alice");
            let mut note = note;
            note.update_content("This is a long content that should be truncated");
            let lua_note = LuaNote::new(note);

            let notes_table: Table = lua.globals().get("notes").unwrap();
            notes_table.set("test_note", lua_note).unwrap();

            let preview: String = lua
                .load("return notes.test_note:preview(20)")
                .eval()
                .unwrap();
            assert!(preview.len() <= 20);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_note_tostring() {
        let lua = setup_lua();
        let s: String = lua
            .load(
                r#"
            local note = notes.Note.new("My Note", "bob")
            return tostring(note)
        "#,
            )
            .eval()
            .unwrap();
        assert!(s.starts_with("Note("));
        assert!(s.contains("My Note"));
    }

    #[test]
    fn test_note_id_short() {
        let lua = setup_lua();
        let (full, short): (String, String) = lua
            .load(
                r#"
            local id = notes.NoteId.new("12345678-90ab-cdef")
            return tostring(id), id:short()
        "#,
            )
            .eval()
            .unwrap();
        assert_eq!(full, "12345678-90ab-cdef");
        assert_eq!(short, "12345678");
    }
}
