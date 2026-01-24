//! Lua runtime wrapper for indras-notes
//!
//! Provides a high-level interface for running Lua scripts with
//! all notes bindings pre-registered.

use mlua::{Lua, Result, Value};
use std::path::Path;
use std::sync::Arc;

use super::register_notes_module;
use crate::app::App;
use crate::log_capture::LogCapture;

/// Lua runtime with indras-notes bindings
pub struct NotesLuaRuntime {
    lua: Lua,
    log_capture: Option<LogCapture>,
}

impl NotesLuaRuntime {
    /// Create a new Lua runtime with all notes bindings registered
    pub fn new() -> Result<Self> {
        Self::with_log_capture(None)
    }

    /// Create a new Lua runtime with log capture enabled
    pub fn with_log_capture(log_capture: Option<LogCapture>) -> Result<Self> {
        let lua = Lua::new();
        register_notes_module(&lua, log_capture.clone())?;
        Ok(Self { lua, log_capture })
    }

    /// Get a reference to the underlying Lua state
    pub fn lua(&self) -> &Lua {
        &self.lua
    }

    /// Get the log capture (if enabled)
    pub fn log_capture(&self) -> Option<&LogCapture> {
        self.log_capture.as_ref()
    }

    /// Set the active app instance for scripts to use
    pub fn set_app(&self, app: Arc<tokio::sync::Mutex<App>>) -> Result<()> {
        use crate::lua::bindings::app::LuaApp;
        let lua_app = LuaApp::new(app);
        self.lua
            .globals()
            .get::<Table>("notes")?
            .set("app", lua_app)?;
        Ok(())
    }

    /// Execute a Lua script from a string
    pub fn exec(&self, script: &str) -> Result<()> {
        self.lua.load(script).exec()
    }

    /// Execute a Lua script and return a value
    pub fn eval<T: mlua::FromLua>(&self, script: &str) -> Result<T> {
        self.lua.load(script).eval()
    }

    /// Execute a Lua script from a file
    pub fn exec_file(&self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        let script = std::fs::read_to_string(path).map_err(|e| {
            mlua::Error::external(format!("Failed to read {}: {}", path.display(), e))
        })?;

        // Set script path for better error messages
        self.lua.scope(|_| {
            self.lua
                .load(&script)
                .set_name(path.to_string_lossy())
                .exec()
        })
    }

    /// Execute a Lua script from a file and return a value
    pub fn eval_file<T: mlua::FromLua>(&self, path: impl AsRef<Path>) -> Result<T> {
        let path = path.as_ref();
        let script = std::fs::read_to_string(path).map_err(|e| {
            mlua::Error::external(format!("Failed to read {}: {}", path.display(), e))
        })?;

        self.lua
            .load(&script)
            .set_name(path.to_string_lossy())
            .eval()
    }

    /// Set a global variable
    pub fn set_global<T: mlua::IntoLua>(&self, name: &str, value: T) -> Result<()> {
        self.lua.globals().set(name, value)
    }

    /// Get a global variable
    pub fn get_global<T: mlua::FromLua>(&self, name: &str) -> Result<T> {
        self.lua.globals().get(name)
    }

    /// Call a Lua function by name
    pub fn call_function<A, R>(&self, name: &str, args: A) -> Result<R>
    where
        A: mlua::IntoLuaMulti,
        R: mlua::FromLuaMulti,
    {
        let func: mlua::Function = self.lua.globals().get(name)?;
        func.call(args)
    }

    /// Create a Lua table
    pub fn create_table(&self) -> Result<mlua::Table> {
        self.lua.create_table()
    }

    /// Check if a value is nil
    pub fn is_nil(&self, name: &str) -> Result<bool> {
        let value: Value = self.lua.globals().get(name)?;
        Ok(matches!(value, Value::Nil))
    }
}

impl Default for NotesLuaRuntime {
    fn default() -> Self {
        Self::new().expect("Failed to create Lua runtime")
    }
}

use mlua::Table;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runtime_creation() {
        let runtime = NotesLuaRuntime::new().unwrap();
        assert!(!runtime.is_nil("notes").unwrap());
    }

    #[test]
    fn test_exec() {
        let runtime = NotesLuaRuntime::new().unwrap();
        runtime.exec("x = 42").unwrap();
        let x: i32 = runtime.get_global("x").unwrap();
        assert_eq!(x, 42);
    }

    #[test]
    fn test_eval() {
        let runtime = NotesLuaRuntime::new().unwrap();
        let result: i32 = runtime.eval("return 1 + 2").unwrap();
        assert_eq!(result, 3);
    }

    #[test]
    fn test_set_get_global() {
        let runtime = NotesLuaRuntime::new().unwrap();
        runtime.set_global("my_value", 123).unwrap();
        let value: i32 = runtime.get_global("my_value").unwrap();
        assert_eq!(value, 123);
    }

    #[test]
    fn test_call_function() {
        let runtime = NotesLuaRuntime::new().unwrap();
        runtime.exec("function add(a, b) return a + b end").unwrap();
        let result: i32 = runtime.call_function("add", (3, 4)).unwrap();
        assert_eq!(result, 7);
    }
}
