//! Lua scripting support for indras-notes
//!
//! Provides Lua bindings for note-taking operations, testing, and automation.
//!
//! # Features
//!
//! - **Note operations**: Create, read, update, delete notes from Lua
//! - **Multi-instance testing**: Create isolated App instances for testing sync
//! - **Structured logging**: JSONL logging from Lua scripts
//! - **Assertions**: Test assertion helpers
//! - **Log analysis**: Query and verify log output
//!
//! # Example
//!
//! ```lua
//! local app = notes.app
//!
//! -- Create notebook and note
//! local nb_id = app:create_notebook("Test")
//! app:open_notebook(nb_id)
//! local note_id = app:create_note("Hello World")
//!
//! notes.log.info("Created note", { note_id = note_id })
//!
//! -- Verify
//! notes.assert.not_nil(app:find_note(note_id:sub(1, 8)))
//! notes.log_assert.no_errors()
//! ```

pub mod bindings;
pub mod runtime;

pub use runtime::NotesLuaRuntime;

use mlua::{Lua, Result};

use crate::log_capture::LogCapture;

/// Register all notes bindings with a Lua state
pub fn register_notes_module(lua: &Lua, log_capture: Option<LogCapture>) -> Result<()> {
    let notes = lua.create_table()?;

    // Register note types (LuaNote, LuaNoteId)
    bindings::note::register(lua, &notes)?;

    // Register notebook binding (LuaNotebook)
    bindings::notebook::register(lua, &notes)?;

    // Register NoteOperation constructors
    bindings::operations::register(lua, &notes)?;

    // Register logging functions
    bindings::logging::register(lua, &notes)?;

    // Register assertion helpers
    bindings::assertions::register(lua, &notes)?;

    // Register log assertions (for verifying log output)
    if let Some(capture) = log_capture {
        bindings::log_assert::register(lua, &notes, capture)?;
    }

    // Register App constructor and type
    bindings::app::register(lua, &notes)?;

    // Register SyncableNotebook for real Automerge sync testing
    bindings::syncable_notebook::register(lua, &notes)?;

    // Set global notes table
    lua.globals().set("notes", notes)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_module() {
        let lua = Lua::new();
        register_notes_module(&lua, None).unwrap();

        // Verify notes global exists
        let result: bool = lua.load("notes ~= nil").eval().unwrap();
        assert!(result);
    }

    #[test]
    fn test_logging_available() {
        let lua = Lua::new();
        register_notes_module(&lua, None).unwrap();

        let result: bool = lua.load("notes.log ~= nil").eval().unwrap();
        assert!(result);
    }

    #[test]
    fn test_assertions_available() {
        let lua = Lua::new();
        register_notes_module(&lua, None).unwrap();

        let result: bool = lua.load("notes.assert ~= nil").eval().unwrap();
        assert!(result);
    }
}
