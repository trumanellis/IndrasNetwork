//! Event hook system for Lua scripts
//!
//! Provides a mechanism for Lua scripts to register callbacks
//! that are invoked when specific simulation events occur.

use mlua::{Function, Lua, Result, Table, Value};
use std::collections::HashMap;

use crate::types::NetworkEvent;

use super::bindings::events::network_event_to_table;

/// Registry for event hooks
#[derive(Default)]
pub struct HookRegistry {
    /// Callbacks for tick events (tick number -> callbacks)
    tick_hooks: Vec<HookCallback>,
    /// Callbacks for specific event types
    event_hooks: HashMap<String, Vec<HookCallback>>,
}

/// A stored callback reference
struct HookCallback {
    /// The Lua function reference key
    key: mlua::RegistryKey,
}

impl HookRegistry {
    /// Create a new empty hook registry
    pub fn new() -> Self {
        Self {
            tick_hooks: Vec::new(),
            event_hooks: HashMap::new(),
        }
    }

    /// Register a tick callback
    pub fn on_tick(&mut self, lua: &Lua, callback: Function) -> Result<()> {
        let key = lua.create_registry_value(callback)?;
        self.tick_hooks.push(HookCallback { key });
        Ok(())
    }

    /// Register an event callback
    pub fn on_event(&mut self, lua: &Lua, event_type: &str, callback: Function) -> Result<()> {
        let key = lua.create_registry_value(callback)?;
        self.event_hooks
            .entry(event_type.to_string())
            .or_default()
            .push(HookCallback { key });
        Ok(())
    }

    /// Fire all tick hooks
    pub fn fire_tick(&self, lua: &Lua, tick: u64) -> Result<()> {
        for hook in &self.tick_hooks {
            let func: Function = lua.registry_value(&hook.key)?;
            func.call::<()>(tick)?;
        }
        Ok(())
    }

    /// Fire hooks for a specific event
    pub fn fire_event(&self, lua: &Lua, event: &NetworkEvent) -> Result<()> {
        let event_type = event_type_name(event);

        if let Some(hooks) = self.event_hooks.get(event_type) {
            let event_table = network_event_to_table(lua, event)?;

            for hook in hooks {
                let func: Function = lua.registry_value(&hook.key)?;
                func.call::<()>(event_table.clone())?;
            }
        }

        Ok(())
    }

    /// Clear all hooks
    pub fn clear(&mut self, lua: &Lua) {
        for hook in self.tick_hooks.drain(..) {
            let _ = lua.remove_registry_value(hook.key);
        }
        for (_, hooks) in self.event_hooks.drain() {
            for hook in hooks {
                let _ = lua.remove_registry_value(hook.key);
            }
        }
    }

    /// Check if there are any registered hooks
    pub fn has_hooks(&self) -> bool {
        !self.tick_hooks.is_empty() || !self.event_hooks.is_empty()
    }
}

/// Get the type name of a NetworkEvent
fn event_type_name(event: &NetworkEvent) -> &'static str {
    match event {
        NetworkEvent::Awake { .. } => "Awake",
        NetworkEvent::Sleep { .. } => "Sleep",
        NetworkEvent::Send { .. } => "Send",
        NetworkEvent::Relay { .. } => "Relay",
        NetworkEvent::Delivered { .. } => "Delivered",
        NetworkEvent::BackProp { .. } => "BackProp",
        NetworkEvent::Dropped { .. } => "Dropped",
        // PQ crypto events
        NetworkEvent::PQSignatureCreated { .. } => "PQSignatureCreated",
        NetworkEvent::PQSignatureVerified { .. } => "PQSignatureVerified",
        NetworkEvent::KEMEncapsulation { .. } => "KEMEncapsulation",
        NetworkEvent::KEMDecapsulation { .. } => "KEMDecapsulation",
        NetworkEvent::InviteCreated { .. } => "InviteCreated",
        NetworkEvent::InviteAccepted { .. } => "InviteAccepted",
        NetworkEvent::InviteFailed { .. } => "InviteFailed",
    }
}

/// Create hook registration methods for a simulation userdata
pub fn create_hook_methods(lua: &Lua, sim_table: &Table) -> Result<()> {
    // Store hook registry in the Lua registry
    let registry = lua.create_table()?;
    registry.set("tick_hooks", lua.create_table()?)?;
    registry.set("event_hooks", lua.create_table()?)?;
    let registry_key = lua.create_registry_value(registry)?;

    // on_tick(callback)
    let key_clone = lua.create_registry_value(lua.registry_value::<Table>(&registry_key)?)?;
    sim_table.set(
        "on_tick",
        lua.create_function(move |lua, (this, callback): (Table, Function)| {
            let registry: Table = lua.registry_value(&key_clone)?;
            let tick_hooks: Table = registry.get("tick_hooks")?;
            let len = tick_hooks.len()?;
            tick_hooks.set(len + 1, callback)?;

            // Return self for chaining
            Ok(this)
        })?,
    )?;

    // on_event(event_type, callback)
    let key_clone = lua.create_registry_value(lua.registry_value::<Table>(&registry_key)?)?;
    sim_table.set(
        "on_event",
        lua.create_function(move |lua, (this, event_type, callback): (Table, String, Function)| {
            let registry: Table = lua.registry_value(&key_clone)?;
            let event_hooks: Table = registry.get("event_hooks")?;

            // Get or create the table for this event type
            let hooks: Table = match event_hooks.get::<Value>(event_type.as_str())? {
                Value::Table(t) => t,
                _ => {
                    let t = lua.create_table()?;
                    event_hooks.set(event_type.as_str(), t.clone())?;
                    t
                }
            };

            let len = hooks.len()?;
            hooks.set(len + 1, callback)?;

            // Return self for chaining
            Ok(this)
        })?,
    )?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::PeerId;

    #[test]
    fn test_hook_registry_creation() {
        let registry = HookRegistry::new();
        assert!(!registry.has_hooks());
    }

    #[test]
    fn test_tick_hook() {
        let lua = Lua::new();
        let mut registry = HookRegistry::new();

        // Create a callback that increments a counter
        lua.load("counter = 0").exec().unwrap();
        let callback: Function = lua
            .load("function(tick) counter = counter + tick end")
            .eval()
            .unwrap();

        registry.on_tick(&lua, callback).unwrap();
        assert!(registry.has_hooks());

        // Fire the hook
        registry.fire_tick(&lua, 5).unwrap();

        let counter: i32 = lua.load("return counter").eval().unwrap();
        assert_eq!(counter, 5);
    }

    #[test]
    fn test_event_hook() {
        let lua = Lua::new();
        let mut registry = HookRegistry::new();

        // Create a callback that stores the peer
        lua.load("last_peer = nil").exec().unwrap();
        let callback: Function = lua
            .load("function(event) last_peer = event.peer end")
            .eval()
            .unwrap();

        registry.on_event(&lua, "Awake", callback).unwrap();

        // Fire an awake event
        let event = NetworkEvent::Awake {
            peer: PeerId('B'),
            tick: 10,
        };
        registry.fire_event(&lua, &event).unwrap();

        let peer: String = lua.load("return last_peer").eval().unwrap();
        assert_eq!(peer, "B");
    }

    #[test]
    fn test_clear_hooks() {
        let lua = Lua::new();
        let mut registry = HookRegistry::new();

        let callback: Function = lua.load("function() end").eval().unwrap();
        registry.on_tick(&lua, callback).unwrap();
        assert!(registry.has_hooks());

        registry.clear(&lua);
        assert!(!registry.has_hooks());
    }
}
