//! Lua runtime wrapper for IndrasNetwork simulation
//!
//! Provides a high-level interface for running Lua scripts with
//! all simulation bindings pre-registered.

use mlua::{Lua, Result, Value};
use std::path::Path;

use super::register_indras_module;

/// Lua runtime with IndrasNetwork bindings
pub struct LuaRuntime {
    lua: Lua,
}

impl LuaRuntime {
    /// Create a new Lua runtime with all indras bindings registered
    pub fn new() -> Result<Self> {
        let lua = Lua::new();
        register_indras_module(&lua)?;
        Ok(Self { lua })
    }

    /// Get a reference to the underlying Lua state
    pub fn lua(&self) -> &Lua {
        &self.lua
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

        // Add script's directory and current working directory to package.path
        // for require() to find local modules
        let mut paths = vec!["./?.lua".to_string(), "./?/init.lua".to_string()];
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                let parent_str = parent.to_string_lossy();
                paths.push(format!("{}/?.lua", parent_str));
                paths.push(format!("{}/?/init.lua", parent_str));
            }
        }
        let add_path = format!(
            "package.path = '{}' .. ';' .. package.path",
            paths.join(";")
        );
        self.lua.load(&add_path).exec()?;

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

impl Default for LuaRuntime {
    fn default() -> Self {
        Self::new().expect("Failed to create Lua runtime")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runtime_creation() {
        let runtime = LuaRuntime::new().unwrap();
        assert!(!runtime.is_nil("indras").unwrap());
    }

    #[test]
    fn test_exec() {
        let runtime = LuaRuntime::new().unwrap();
        runtime.exec("x = 42").unwrap();
        let x: i32 = runtime.get_global("x").unwrap();
        assert_eq!(x, 42);
    }

    #[test]
    fn test_eval() {
        let runtime = LuaRuntime::new().unwrap();
        let result: i32 = runtime.eval("return 1 + 2").unwrap();
        assert_eq!(result, 3);
    }

    #[test]
    fn test_set_get_global() {
        let runtime = LuaRuntime::new().unwrap();
        runtime.set_global("my_value", 123).unwrap();
        let value: i32 = runtime.get_global("my_value").unwrap();
        assert_eq!(value, 123);
    }

    #[test]
    fn test_call_function() {
        let runtime = LuaRuntime::new().unwrap();
        runtime.exec("function add(a, b) return a + b end").unwrap();
        let result: i32 = runtime.call_function("add", (3, 4)).unwrap();
        assert_eq!(result, 7);
    }

    #[test]
    fn test_full_scenario() {
        let runtime = LuaRuntime::new().unwrap();

        let result: i32 = runtime
            .eval(
                r#"
            local mesh = indras.MeshBuilder.new(3):full_mesh()
            local sim = indras.Simulation.new(mesh, indras.SimConfig.manual())

            sim:force_online(indras.PeerId.new('A'))
            sim:force_online(indras.PeerId.new('B'))

            sim:send_message(
                indras.PeerId.new('A'),
                indras.PeerId.new('B'),
                "Hello"
            )

            sim:run_ticks(5)

            return sim.stats.messages_delivered
        "#,
            )
            .unwrap();

        assert_eq!(result, 1);
    }
}
