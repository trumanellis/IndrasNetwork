//! Lua bindings for PRoPHET routing protocol
//!
//! Provides Lua wrappers for querying PRoPHET routing state from the simulation.

use mlua::{Lua, Result, Table};

/// Register PRoPHET routing functions
///
/// Note: PRoPHET state is already integrated into PeerState in the simulation.
/// The methods for accessing it are added directly to LuaSimulation in simulation.rs.
pub fn register(_lua: &Lua, _indras: &Table) -> Result<()> {
    // PRoPHET methods are added directly to LuaSimulation
    // No separate Prophet type needed since it's integrated into peers
    Ok(())
}
