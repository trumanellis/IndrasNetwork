//! Lua bindings for Notebook type
//!
//! Provides UserData implementation for Notebook.

use mlua::{Lua, Result, Table, UserData, UserDataMethods};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::notebook::Notebook;

use super::note::LuaNote;
use super::operations::LuaNoteOperation;

/// Lua wrapper for Notebook
pub struct LuaNotebook {
    inner: Arc<Mutex<Notebook>>,
}

impl LuaNotebook {
    pub fn new(notebook: Notebook) -> Self {
        Self {
            inner: Arc::new(Mutex::new(notebook)),
        }
    }

    pub fn from_arc(notebook: Arc<Mutex<Notebook>>) -> Self {
        Self { inner: notebook }
    }
}

impl Clone for LuaNotebook {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl UserData for LuaNotebook {
    fn add_fields<F: mlua::UserDataFields<Self>>(fields: &mut F) {
        // Read-only fields
        fields.add_field_method_get("name", |_, this| {
            let notebook = futures::executor::block_on(this.inner.lock());
            Ok(notebook.name.clone())
        });

        fields.add_field_method_get("note_count", |_, this| {
            let notebook = futures::executor::block_on(this.inner.lock());
            Ok(notebook.count())
        });

        fields.add_field_method_get("interface_id", |_, this| {
            let notebook = futures::executor::block_on(this.inner.lock());
            Ok(hex::encode(notebook.interface_id.as_bytes()))
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // list() -> array of Notes
        methods.add_method("list", |lua, this, ()| {
            let notebook = futures::executor::block_on(this.inner.lock());
            let notes = notebook.list();

            let table = lua.create_table()?;
            for (i, note) in notes.iter().enumerate() {
                table.set(i + 1, LuaNote::new((*note).clone()))?;
            }
            Ok(table)
        });

        // get(id) -> Note or nil
        methods.add_method("get", |_, this, id: String| {
            let notebook = futures::executor::block_on(this.inner.lock());
            Ok(notebook.get(&id).map(|n| LuaNote::new(n.clone())))
        });

        // find(partial_id) -> Note or nil
        methods.add_method("find", |_, this, partial_id: String| {
            let notebook = futures::executor::block_on(this.inner.lock());

            // Try exact match first
            if let Some(note) = notebook.get(&partial_id) {
                return Ok(Some(LuaNote::new(note.clone())));
            }

            // Try prefix match
            for note in notebook.notes.values() {
                if note.id.starts_with(&partial_id) {
                    return Ok(Some(LuaNote::new(note.clone())));
                }
            }

            Ok(None)
        });

        // apply(operation) -> note_id or nil
        methods.add_method("apply", |_, this, op: mlua::AnyUserData| {
            let op = op.borrow::<LuaNoteOperation>()?;
            let mut notebook = futures::executor::block_on(this.inner.lock());
            Ok(notebook.apply(op.inner().clone()))
        });

        // is_empty() -> bool
        methods.add_method("is_empty", |_, this, ()| {
            let notebook = futures::executor::block_on(this.inner.lock());
            Ok(notebook.is_empty())
        });

        // tostring metamethod
        methods.add_meta_method(mlua::MetaMethod::ToString, |_, this, ()| {
            let notebook = futures::executor::block_on(this.inner.lock());
            Ok(format!(
                "Notebook({}: {} notes)",
                notebook.name,
                notebook.count()
            ))
        });
    }
}

/// Register notebook type with the notes table
pub fn register(lua: &Lua, notes: &Table) -> Result<()> {
    let notebook_table = lua.create_table()?;

    // Notebook.new(name, interface_id_hex) -> Notebook
    notebook_table.set(
        "new",
        lua.create_function(|_, (name, interface_id_hex): (String, String)| {
            let interface_id_bytes = hex::decode(&interface_id_hex)
                .map_err(|e| mlua::Error::external(format!("Invalid interface ID hex: {}", e)))?;

            let interface_id = indras_core::InterfaceId::from_slice(&interface_id_bytes)
                .ok_or_else(|| mlua::Error::external("Invalid interface ID length"))?;

            let notebook = Notebook::new(name, interface_id);
            Ok(LuaNotebook::new(notebook))
        })?,
    )?;

    notes.set("Notebook", notebook_table)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::note::{Note, NoteOperation};
    use indras_core::InterfaceId;

    fn setup_lua() -> Lua {
        let lua = Lua::new();
        let notes = lua.create_table().unwrap();
        register(&lua, &notes).unwrap();
        super::super::note::register(&lua, &notes).unwrap();
        super::super::operations::register(&lua, &notes).unwrap();
        lua.globals().set("notes", notes).unwrap();
        lua
    }

    #[test]
    fn test_notebook_fields() {
        let lua = setup_lua();

        // Create a notebook and add it to Lua
        let interface_id = InterfaceId::generate();
        let notebook = Notebook::new("Test Notebook", interface_id);
        let lua_notebook = LuaNotebook::new(notebook);

        lua.globals()
            .get::<Table>("notes")
            .unwrap()
            .set("test_nb", lua_notebook)
            .unwrap();

        let name: String = lua.load("return notes.test_nb.name").eval().unwrap();
        assert_eq!(name, "Test Notebook");

        let count: usize = lua.load("return notes.test_nb.note_count").eval().unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_notebook_apply_and_list() {
        let lua = setup_lua();

        // Create a notebook
        let interface_id = InterfaceId::generate();
        let notebook = Notebook::new("Test", interface_id);
        let lua_notebook = LuaNotebook::new(notebook);

        lua.globals()
            .get::<Table>("notes")
            .unwrap()
            .set("nb", lua_notebook)
            .unwrap();

        // Apply create operation and list
        let count: usize = lua
            .load(
                r#"
            local note = notes.Note.new("First Note", "alice")
            local op = notes.NoteOperation.create(note)
            notes.nb:apply(op)
            return notes.nb.note_count
        "#,
            )
            .eval()
            .unwrap();
        assert_eq!(count, 1);

        // List notes
        let title: String = lua
            .load(
                r#"
            local list = notes.nb:list()
            return list[1].title
        "#,
            )
            .eval()
            .unwrap();
        assert_eq!(title, "First Note");
    }

    #[test]
    fn test_notebook_find() {
        let lua = setup_lua();

        // Create a notebook with a note
        let interface_id = InterfaceId::generate();
        let mut notebook = Notebook::new("Test", interface_id);
        let note = Note::new("Find Me", "alice");
        let note_id = note.id.clone();
        notebook.apply(NoteOperation::create(note));

        let lua_notebook = LuaNotebook::new(notebook);

        lua.globals()
            .get::<Table>("notes")
            .unwrap()
            .set("nb", lua_notebook)
            .unwrap();

        // Find by partial ID
        let partial = &note_id[..8];
        lua.globals().set("partial_id", partial).unwrap();

        let found_title: String = lua
            .load(
                r#"
            local note = notes.nb:find(partial_id)
            return note.title
        "#,
            )
            .eval()
            .unwrap();
        assert_eq!(found_title, "Find Me");
    }
}
