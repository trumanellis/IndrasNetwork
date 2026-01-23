//! Lua bindings for NoteOperation
//!
//! Provides constructors for creating note operations from Lua.

use mlua::{Lua, Result, Table, UserData, UserDataMethods};

use crate::note::NoteOperation;

use super::note::LuaNote;

/// Lua wrapper for NoteOperation
#[derive(Clone)]
pub struct LuaNoteOperation {
    inner: NoteOperation,
}

impl LuaNoteOperation {
    pub fn new(op: NoteOperation) -> Self {
        Self { inner: op }
    }

    pub fn inner(&self) -> &NoteOperation {
        &self.inner
    }

    pub fn into_inner(self) -> NoteOperation {
        self.inner
    }
}

impl UserData for LuaNoteOperation {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // tostring metamethod
        methods.add_meta_method(mlua::MetaMethod::ToString, |_, this, ()| {
            let desc = match &this.inner {
                NoteOperation::Create(note) => format!("Create({})", note.title),
                NoteOperation::UpdateContent { id, .. } => {
                    format!("UpdateContent({})", &id[..8.min(id.len())])
                }
                NoteOperation::UpdateTitle { id, title } => {
                    format!("UpdateTitle({}, {})", &id[..8.min(id.len())], title)
                }
                NoteOperation::Delete { id } => format!("Delete({})", &id[..8.min(id.len())]),
            };
            Ok(format!("NoteOperation::{}", desc))
        });
    }
}

/// Register NoteOperation constructors with the notes table
pub fn register(lua: &Lua, notes: &Table) -> Result<()> {
    let op_table = lua.create_table()?;

    // NoteOperation.create(note) -> NoteOperation
    op_table.set(
        "create",
        lua.create_function(|_, note: mlua::AnyUserData| {
            let note = note.borrow::<LuaNote>()?;
            Ok(LuaNoteOperation::new(NoteOperation::create(
                note.inner().clone(),
            )))
        })?,
    )?;

    // NoteOperation.update_content(id, content) -> NoteOperation
    op_table.set(
        "update_content",
        lua.create_function(|_, (id, content): (String, String)| {
            Ok(LuaNoteOperation::new(NoteOperation::update_content(
                id, content,
            )))
        })?,
    )?;

    // NoteOperation.update_title(id, title) -> NoteOperation
    op_table.set(
        "update_title",
        lua.create_function(|_, (id, title): (String, String)| {
            Ok(LuaNoteOperation::new(NoteOperation::update_title(
                id, title,
            )))
        })?,
    )?;

    // NoteOperation.delete(id) -> NoteOperation
    op_table.set(
        "delete",
        lua.create_function(|_, id: String| Ok(LuaNoteOperation::new(NoteOperation::delete(id))))?,
    )?;

    notes.set("NoteOperation", op_table)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_lua() -> Lua {
        let lua = Lua::new();
        let notes = lua.create_table().unwrap();
        register(&lua, &notes).unwrap();
        super::super::note::register(&lua, &notes).unwrap();
        lua.globals().set("notes", notes).unwrap();
        lua
    }

    #[test]
    fn test_create_operation() {
        let lua = setup_lua();
        let s: String = lua
            .load(
                r#"
            local note = notes.Note.new("Test Note", "alice")
            local op = notes.NoteOperation.create(note)
            return tostring(op)
        "#,
            )
            .eval()
            .unwrap();
        assert!(s.contains("Create"));
        assert!(s.contains("Test Note"));
    }

    #[test]
    fn test_update_content_operation() {
        let lua = setup_lua();
        let s: String = lua
            .load(
                r#"
            local op = notes.NoteOperation.update_content("12345678-abcd", "new content")
            return tostring(op)
        "#,
            )
            .eval()
            .unwrap();
        assert!(s.contains("UpdateContent"));
        assert!(s.contains("12345678"));
    }

    #[test]
    fn test_update_title_operation() {
        let lua = setup_lua();
        let s: String = lua
            .load(
                r#"
            local op = notes.NoteOperation.update_title("12345678-abcd", "New Title")
            return tostring(op)
        "#,
            )
            .eval()
            .unwrap();
        assert!(s.contains("UpdateTitle"));
        assert!(s.contains("New Title"));
    }

    #[test]
    fn test_delete_operation() {
        let lua = setup_lua();
        let s: String = lua
            .load(
                r#"
            local op = notes.NoteOperation.delete("12345678-abcd")
            return tostring(op)
        "#,
            )
            .eval()
            .unwrap();
        assert!(s.contains("Delete"));
    }
}
