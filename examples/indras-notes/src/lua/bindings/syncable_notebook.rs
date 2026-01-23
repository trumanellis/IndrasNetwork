//! Lua bindings for SyncableNotebook type
//!
//! Provides UserData implementation for SyncableNotebook with real Automerge sync.

use mlua::{Lua, Result, Table, UserData, UserDataMethods};
use std::sync::Arc;
use tokio::sync::Mutex;

use indras_core::{InterfaceId, SimulationIdentity};

use crate::syncable_notebook::SyncableNotebook;

use super::note::LuaNote;
use super::operations::LuaNoteOperation;

/// Lua wrapper for SyncableNotebook with real Automerge sync
pub struct LuaSyncableNotebook {
    inner: Arc<Mutex<SyncableNotebook>>,
}

impl LuaSyncableNotebook {
    pub fn new(notebook: SyncableNotebook) -> Self {
        Self {
            inner: Arc::new(Mutex::new(notebook)),
        }
    }

    pub fn from_arc(notebook: Arc<Mutex<SyncableNotebook>>) -> Self {
        Self { inner: notebook }
    }

    /// Create a forked copy of the notebook for another peer
    pub fn fork(&self, new_peer: SimulationIdentity) -> Self {
        let inner = futures::executor::block_on(self.inner.lock());
        // We need mutable access for fork, so we'll use a different approach
        // Save and reload with new peer
        let bytes = {
            let mut guard = futures::executor::block_on(self.inner.lock());
            guard.save()
        };

        let forked = SyncableNotebook::load(
            inner.name.clone(),
            inner.interface_id,
            new_peer,
            &bytes,
        ).expect("Fork failed");

        Self::new(forked)
    }
}

impl Clone for LuaSyncableNotebook {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl UserData for LuaSyncableNotebook {
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
            for note in notebook.list() {
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

        // ===== Automerge Sync Methods =====

        // heads() -> table of hex strings
        methods.add_method("heads", |lua, this, ()| {
            let mut notebook = futures::executor::block_on(this.inner.lock());
            let heads = notebook.heads_hex();

            let table = lua.create_table()?;
            for (i, head) in heads.iter().enumerate() {
                table.set(i + 1, head.as_str())?;
            }
            Ok(table)
        });

        // generate_sync(their_heads_table) -> sync_message (bytes as hex)
        methods.add_method("generate_sync", |_, this, their_heads: mlua::Table| {
            let mut heads_vec: Vec<String> = Vec::new();

            for pair in their_heads.pairs::<i64, String>() {
                let (_, head) = pair?;
                heads_vec.push(head);
            }

            let mut notebook = futures::executor::block_on(this.inner.lock());
            let sync_msg = notebook.generate_sync_message_hex(&heads_vec);

            Ok(hex::encode(&sync_msg))
        });

        // apply_sync(sync_message_hex) -> bool (whether changes were applied)
        methods.add_method("apply_sync", |_, this, sync_msg_hex: String| {
            let sync_bytes = hex::decode(&sync_msg_hex)
                .map_err(|e| mlua::Error::external(format!("Invalid sync message hex: {}", e)))?;

            let mut notebook = futures::executor::block_on(this.inner.lock());
            notebook.apply_sync_message(&sync_bytes)
                .map_err(|e| mlua::Error::external(e))
        });

        // save() -> bytes as hex (for persistence)
        methods.add_method("save", |_, this, ()| {
            let mut notebook = futures::executor::block_on(this.inner.lock());
            Ok(hex::encode(notebook.save()))
        });

        // tostring metamethod
        methods.add_meta_method(mlua::MetaMethod::ToString, |_, this, ()| {
            let notebook = futures::executor::block_on(this.inner.lock());
            Ok(format!(
                "SyncableNotebook({}: {} notes)",
                notebook.name,
                notebook.count()
            ))
        });
    }
}

/// Register syncable notebook type with the notes table
pub fn register(lua: &Lua, notes: &Table) -> Result<()> {
    let syncable_notebook_table = lua.create_table()?;

    // SyncableNotebook.new(name, peer_name) -> SyncableNotebook
    syncable_notebook_table.set(
        "new",
        lua.create_function(|_, (name, peer_name): (String, String)| {
            // Create a simulation identity from the peer name (must be uppercase letter)
            let first_char = peer_name.chars().next().unwrap_or('A').to_ascii_uppercase();
            let peer = SimulationIdentity::new(first_char)
                .ok_or_else(|| mlua::Error::external("Peer name must start with a letter"))?;

            let interface_id = InterfaceId::generate();
            let notebook = SyncableNotebook::new(name, interface_id, peer);
            Ok(LuaSyncableNotebook::new(notebook))
        })?,
    )?;

    // SyncableNotebook.new_with_id(name, interface_id_hex, peer_name) -> SyncableNotebook
    syncable_notebook_table.set(
        "new_with_id",
        lua.create_function(|_, (name, interface_id_hex, peer_name): (String, String, String)| {
            let interface_id_bytes = hex::decode(&interface_id_hex)
                .map_err(|e| mlua::Error::external(format!("Invalid interface ID hex: {}", e)))?;

            let interface_id = InterfaceId::from_slice(&interface_id_bytes)
                .ok_or_else(|| mlua::Error::external("Invalid interface ID length"))?;

            let first_char = peer_name.chars().next().unwrap_or('A').to_ascii_uppercase();
            let peer = SimulationIdentity::new(first_char)
                .ok_or_else(|| mlua::Error::external("Peer name must start with a letter"))?;

            let notebook = SyncableNotebook::new(name, interface_id, peer);
            Ok(LuaSyncableNotebook::new(notebook))
        })?,
    )?;

    // SyncableNotebook.load(name, interface_id_hex, peer_name, data_hex) -> SyncableNotebook
    syncable_notebook_table.set(
        "load",
        lua.create_function(|_, (name, interface_id_hex, peer_name, data_hex): (String, String, String, String)| {
            let interface_id_bytes = hex::decode(&interface_id_hex)
                .map_err(|e| mlua::Error::external(format!("Invalid interface ID hex: {}", e)))?;

            let interface_id = InterfaceId::from_slice(&interface_id_bytes)
                .ok_or_else(|| mlua::Error::external("Invalid interface ID length"))?;

            let first_char = peer_name.chars().next().unwrap_or('A').to_ascii_uppercase();
            let peer = SimulationIdentity::new(first_char)
                .ok_or_else(|| mlua::Error::external("Peer name must start with a letter"))?;

            let data = hex::decode(&data_hex)
                .map_err(|e| mlua::Error::external(format!("Invalid data hex: {}", e)))?;

            let notebook = SyncableNotebook::load(name, interface_id, peer, &data)
                .map_err(|e| mlua::Error::external(e))?;

            Ok(LuaSyncableNotebook::new(notebook))
        })?,
    )?;

    // SyncableNotebook.fork(notebook, new_peer_name) -> SyncableNotebook
    syncable_notebook_table.set(
        "fork",
        lua.create_function(|_, (notebook, new_peer_name): (mlua::AnyUserData, String)| {
            let lua_nb = notebook.borrow::<LuaSyncableNotebook>()?;

            let first_char = new_peer_name.chars().next().unwrap_or('B').to_ascii_uppercase();
            let new_peer = SimulationIdentity::new(first_char)
                .ok_or_else(|| mlua::Error::external("Peer name must start with a letter"))?;

            // Save the current notebook and load as new peer
            let (name, interface_id, bytes) = {
                let mut guard = futures::executor::block_on(lua_nb.inner.lock());
                (guard.name.clone(), guard.interface_id, guard.save())
            };

            let forked = SyncableNotebook::load(name, interface_id, new_peer, &bytes)
                .map_err(|e| mlua::Error::external(e))?;

            Ok(LuaSyncableNotebook::new(forked))
        })?,
    )?;

    notes.set("SyncableNotebook", syncable_notebook_table)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::note::Note;

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
    fn test_syncable_notebook_creation() {
        let lua = setup_lua();

        let code = r#"
            local nb = notes.SyncableNotebook.new("Test", "Alice")
            return nb.name, nb.note_count
        "#;

        let (name, count): (String, usize) = lua.load(code).eval().unwrap();
        assert_eq!(name, "Test");
        assert_eq!(count, 0);
    }

    #[test]
    fn test_syncable_notebook_sync() {
        let lua = setup_lua();

        // Create Alice's notebook, add a note, fork to Bob, sync
        let result: bool = lua.load(r#"
            -- Create Alice's notebook
            local alice = notes.SyncableNotebook.new("Shared", "Alice")

            -- Alice creates a note
            local note = notes.Note.new("Alice's Note", "alice")
            local op = notes.NoteOperation.create(note)
            alice:apply(op)

            -- Fork to Bob (simulates initial sync)
            local bob = notes.SyncableNotebook.fork(alice, "Bob")

            -- Verify Bob has the note
            if bob.note_count ~= 1 then
                return false
            end

            -- Alice adds another note
            local note2 = notes.Note.new("Alice's Second Note", "alice")
            local op2 = notes.NoteOperation.create(note2)
            alice:apply(op2)

            -- Bob doesn't have it yet
            if bob.note_count ~= 1 then
                return false
            end

            -- Sync: Get Bob's heads, generate sync from Alice
            local bob_heads = bob:heads()
            local sync_msg = alice:generate_sync(bob_heads)

            -- Apply sync to Bob
            local changed = bob:apply_sync(sync_msg)

            -- Bob should now have both notes
            return bob.note_count == 2 and changed
        "#).eval().unwrap();

        assert!(result, "Sync should work correctly");
    }

    #[test]
    fn test_bidirectional_sync() {
        let lua = setup_lua();

        // Test concurrent edits merging correctly
        let result: bool = lua.load(r#"
            -- Create Alice's notebook
            local alice = notes.SyncableNotebook.new("Shared", "Alice")

            -- Fork to Bob
            local bob = notes.SyncableNotebook.fork(alice, "Bob")

            -- Get initial heads before concurrent edits
            local alice_initial_heads = alice:heads()
            local bob_initial_heads = bob:heads()

            -- Alice makes an edit
            local alice_note = notes.Note.new("Alice's Concurrent Note", "alice")
            alice:apply(notes.NoteOperation.create(alice_note))

            -- Bob makes a concurrent edit
            local bob_note = notes.Note.new("Bob's Concurrent Note", "bob")
            bob:apply(notes.NoteOperation.create(bob_note))

            -- Before sync, each has only their own note
            if alice.note_count ~= 1 or bob.note_count ~= 1 then
                return false
            end

            -- Sync Alice -> Bob
            local sync_to_bob = alice:generate_sync(bob_initial_heads)
            bob:apply_sync(sync_to_bob)

            -- Sync Bob -> Alice
            local sync_to_alice = bob:generate_sync(alice_initial_heads)
            alice:apply_sync(sync_to_alice)

            -- Both should have both notes (CRDT convergence)
            return alice.note_count == 2 and bob.note_count == 2
        "#).eval().unwrap();

        assert!(result, "Bidirectional sync should result in both notebooks having both notes");
    }
}
