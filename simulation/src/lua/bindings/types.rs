//! Lua bindings for core simulation types
//!
//! Provides Lua wrappers for PeerId, PacketId, and Priority.

use mlua::{FromLua, Lua, MetaMethod, Result, Table, UserData, UserDataMethods, Value};

use crate::types::PeerId;

/// Lua wrapper for PeerId
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LuaPeerId(pub PeerId);

impl LuaPeerId {
    pub fn inner(&self) -> PeerId {
        self.0
    }
}

impl From<PeerId> for LuaPeerId {
    fn from(peer_id: PeerId) -> Self {
        Self(peer_id)
    }
}

impl From<LuaPeerId> for PeerId {
    fn from(lua_peer_id: LuaPeerId) -> Self {
        lua_peer_id.0
    }
}

impl FromLua for LuaPeerId {
    fn from_lua(value: Value, _lua: &Lua) -> Result<Self> {
        match value {
            Value::UserData(ud) => ud.borrow::<Self>().map(|v| *v),
            Value::String(s) => {
                let str_val = s.to_str()?;
                let c = str_val.chars().next().ok_or_else(|| {
                    mlua::Error::external("PeerId requires a single character")
                })?;
                let peer_id = PeerId::new(c).ok_or_else(|| {
                    mlua::Error::external(format!("Invalid PeerId: '{}'. Must be A-Z", c))
                })?;
                Ok(LuaPeerId(peer_id))
            }
            _ => Err(mlua::Error::external("Expected PeerId userdata or string")),
        }
    }
}

impl UserData for LuaPeerId {
    fn add_fields<F: mlua::UserDataFields<Self>>(fields: &mut F) {
        // Read the underlying character
        fields.add_field_method_get("char", |_, this| Ok(this.0.0.to_string()));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // Equality comparison
        methods.add_meta_method(MetaMethod::Eq, |_, this, other: LuaPeerId| {
            Ok(this.0 == other.0)
        });

        // String conversion
        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            Ok(this.0.to_string())
        });

        // Ordering
        methods.add_meta_method(MetaMethod::Lt, |_, this, other: LuaPeerId| {
            Ok(this.0 < other.0)
        });

        methods.add_meta_method(MetaMethod::Le, |_, this, other: LuaPeerId| {
            Ok(this.0 <= other.0)
        });
    }
}

/// Priority level for messages
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LuaPriority {
    Low,
    Normal,
    High,
    Critical,
}

impl FromLua for LuaPriority {
    fn from_lua(value: Value, _lua: &Lua) -> Result<Self> {
        match value {
            Value::UserData(ud) => ud.borrow::<Self>().map(|v| *v),
            Value::String(s) => {
                let str_val: &str = &s.to_str()?;
                match str_val {
                    "low" => Ok(LuaPriority::Low),
                    "normal" => Ok(LuaPriority::Normal),
                    "high" => Ok(LuaPriority::High),
                    "critical" => Ok(LuaPriority::Critical),
                    other => Err(mlua::Error::external(format!("Unknown priority: {}", other))),
                }
            }
            _ => Err(mlua::Error::external("Expected Priority userdata or string")),
        }
    }
}

impl UserData for LuaPriority {
    fn add_fields<F: mlua::UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("value", |_, this| {
            Ok(match this {
                LuaPriority::Low => "low",
                LuaPriority::Normal => "normal",
                LuaPriority::High => "high",
                LuaPriority::Critical => "critical",
            })
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            Ok(match this {
                LuaPriority::Low => "low",
                LuaPriority::Normal => "normal",
                LuaPriority::High => "high",
                LuaPriority::Critical => "critical",
            })
        });

        methods.add_meta_method(MetaMethod::Eq, |_, this, other: LuaPriority| {
            Ok(*this == other)
        });
    }
}

/// Register type constructors with the indras table
pub fn register(lua: &Lua, indras: &Table) -> Result<()> {
    // PeerId constructor table
    let peer_id = lua.create_table()?;

    // PeerId.new(char) - create from a single character
    peer_id.set(
        "new",
        lua.create_function(|_, c: String| {
            let c = c.chars().next().ok_or_else(|| {
                mlua::Error::external("PeerId requires a single character")
            })?;
            let peer_id = PeerId::new(c).ok_or_else(|| {
                mlua::Error::external(format!("Invalid PeerId: '{}'. Must be A-Z", c))
            })?;
            Ok(LuaPeerId(peer_id))
        })?,
    )?;

    // PeerId.range_to(char) - generate A..=char
    peer_id.set(
        "range_to",
        lua.create_function(|_, end: String| {
            let end = end.chars().next().ok_or_else(|| {
                mlua::Error::external("range_to requires a single character")
            })?;
            let peers: Vec<LuaPeerId> = PeerId::range_to(end)
                .into_iter()
                .map(LuaPeerId)
                .collect();
            Ok(peers)
        })?,
    )?;

    indras.set("PeerId", peer_id)?;

    // Priority constructor table
    let priority = lua.create_table()?;

    priority.set("low", lua.create_function(|_, ()| Ok(LuaPriority::Low))?)?;
    priority.set("normal", lua.create_function(|_, ()| Ok(LuaPriority::Normal))?)?;
    priority.set("high", lua.create_function(|_, ()| Ok(LuaPriority::High))?)?;
    priority.set("critical", lua.create_function(|_, ()| Ok(LuaPriority::Critical))?)?;

    indras.set("Priority", priority)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_peer_id_new() {
        let lua = Lua::new();
        let indras = lua.create_table().unwrap();
        register(&lua, &indras).unwrap();
        lua.globals().set("indras", indras).unwrap();

        let result: String = lua
            .load(r#"
                local peer = indras.PeerId.new('A')
                return tostring(peer)
            "#)
            .eval()
            .unwrap();
        assert_eq!(result, "A");
    }

    #[test]
    fn test_peer_id_invalid() {
        let lua = Lua::new();
        let indras = lua.create_table().unwrap();
        register(&lua, &indras).unwrap();
        lua.globals().set("indras", indras).unwrap();

        let result = lua
            .load(r#"indras.PeerId.new('a')"#)
            .exec();
        assert!(result.is_err());
    }

    #[test]
    fn test_peer_id_range() {
        let lua = Lua::new();
        let indras = lua.create_table().unwrap();
        register(&lua, &indras).unwrap();
        lua.globals().set("indras", indras).unwrap();

        let result: i32 = lua
            .load(r#"
                local peers = indras.PeerId.range_to('C')
                return #peers
            "#)
            .eval()
            .unwrap();
        assert_eq!(result, 3);
    }

    #[test]
    fn test_peer_id_equality() {
        let lua = Lua::new();
        let indras = lua.create_table().unwrap();
        register(&lua, &indras).unwrap();
        lua.globals().set("indras", indras).unwrap();

        let result: bool = lua
            .load(r#"
                local a1 = indras.PeerId.new('A')
                local a2 = indras.PeerId.new('A')
                local b = indras.PeerId.new('B')
                return a1 == a2 and a1 ~= b
            "#)
            .eval()
            .unwrap();
        assert!(result);
    }

    #[test]
    fn test_priority() {
        let lua = Lua::new();
        let indras = lua.create_table().unwrap();
        register(&lua, &indras).unwrap();
        lua.globals().set("indras", indras).unwrap();

        let result: String = lua
            .load(r#"
                local p = indras.Priority.high()
                return tostring(p)
            "#)
            .eval()
            .unwrap();
        assert_eq!(result, "high");
    }
}
