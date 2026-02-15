//! Lua scripting support for IndrasNetwork simulation
//!
//! This module provides Lua bindings for rapid scenario definition,
//! event-driven testing, and parameterized fuzzing without recompilation.
//!
//! # Features
//!
//! - **Structured JSONL logging** from Lua scripts
//! - **Correlation context propagation** for distributed tracing
//! - **Event hooks** for reactive testing
//! - **Assertion helpers** for test scenarios
//!
//! # Example
//!
//! ```lua
//! local indras = require("indras")
//! local ctx = indras.correlation.new_root()
//!
//! indras.log.info("Starting test", { trace_id = ctx.trace_id })
//!
//! local mesh = indras.Mesh.from_edges({{'A','B'}, {'B','C'}})
//! local sim = indras.Simulation.new(mesh, indras.SimConfig.manual())
//!
//! sim:force_online('A')
//! sim:send_message('A', 'C', "Hello")
//! sim:run_ticks(10)
//!
//! indras.assert.eq(sim.stats.messages_delivered, 1)
//! ```

pub mod assertions;
pub mod bindings;
pub mod hooks;
pub mod runtime;

pub use runtime::LuaRuntime;

use mlua::{Lua, Result};

/// Register all indras bindings with a Lua state
pub fn register_indras_module(lua: &Lua) -> Result<()> {
    let indras = lua.create_table()?;

    // Register type constructors
    bindings::types::register(lua, &indras)?;

    // Register Mesh and MeshBuilder
    bindings::mesh::register(lua, &indras)?;

    // Register SimConfig
    bindings::simulation::register_config(lua, &indras)?;

    // Register Simulation constructor
    bindings::simulation::register(lua, &indras)?;

    // Register logging functions
    bindings::logging::register(lua, &indras)?;

    // Register correlation context
    bindings::correlation::register(lua, &indras)?;

    // Register event constants
    bindings::events::register(lua, &indras)?;

    // Register PRoPHET routing
    bindings::routing::register(lua, &indras)?;

    // Register IoT bindings (duty cycling, compact messages, memory tracking)
    bindings::iot::register(lua, &indras)?;

    // Register SyncEngine bindings
    bindings::sync_engine::register(lua, &indras)?;

    // Register pass story authentication bindings
    bindings::pass_story::register(lua, &indras)?;

    // Register LiveNode bindings (real P2P nodes)
    bindings::live_node::register(lua, &indras)?;

    // Register assertion helpers
    assertions::register(lua, &indras)?;

    // Register async sleep (needed for live node sync waits)
    indras.set(
        "sleep",
        lua.create_async_function(|_, secs: f64| async move {
            tokio::time::sleep(std::time::Duration::from_secs_f64(secs)).await;
            Ok(())
        })?,
    )?;

    // Set global indras table
    lua.globals().set("indras", indras)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_module() {
        let lua = Lua::new();
        register_indras_module(&lua).unwrap();

        // Verify indras global exists
        let result: bool = lua.load("indras ~= nil").eval().unwrap();
        assert!(result);
    }

    #[test]
    fn test_peer_id_creation() {
        let lua = Lua::new();
        register_indras_module(&lua).unwrap();

        let result: String = lua
            .load(
                r#"
                local peer = indras.PeerId.new('A')
                return tostring(peer)
            "#,
            )
            .eval()
            .unwrap();
        assert_eq!(result, "A");
    }

    #[test]
    fn test_mesh_builder() {
        let lua = Lua::new();
        register_indras_module(&lua).unwrap();

        let result: usize = lua
            .load(
                r#"
                local mesh = indras.MeshBuilder.new(3):full_mesh()
                return mesh:peer_count()
            "#,
            )
            .eval()
            .unwrap();
        assert_eq!(result, 3);
    }
}
