//! Lua bindings for NetworkEvent
//!
//! Provides Lua access to network events for introspection and hooks.

use mlua::{Lua, Result, Table};

use crate::types::{DropReason, NetworkEvent};

/// Convert a NetworkEvent to a Lua table
pub fn network_event_to_table(lua: &Lua, event: &NetworkEvent) -> Result<Table> {
    let t = lua.create_table()?;

    match event {
        NetworkEvent::Awake { peer, tick } => {
            t.set("type", "Awake")?;
            t.set("peer", peer.to_string())?;
            t.set("tick", *tick)?;
        }
        NetworkEvent::Sleep { peer, tick } => {
            t.set("type", "Sleep")?;
            t.set("peer", peer.to_string())?;
            t.set("tick", *tick)?;
        }
        NetworkEvent::Send { from, to, payload, tick } => {
            t.set("type", "Send")?;
            t.set("from", from.to_string())?;
            t.set("to", to.to_string())?;
            t.set("payload_len", payload.len())?;
            t.set("tick", *tick)?;
        }
        NetworkEvent::Relay { from, via, to, packet_id, tick } => {
            t.set("type", "Relay")?;
            t.set("from", from.to_string())?;
            t.set("via", via.to_string())?;
            t.set("to", to.to_string())?;
            t.set("packet_id", packet_id.to_string())?;
            t.set("tick", *tick)?;
        }
        NetworkEvent::Delivered { packet_id, to, tick } => {
            t.set("type", "Delivered")?;
            t.set("packet_id", packet_id.to_string())?;
            t.set("to", to.to_string())?;
            t.set("tick", *tick)?;
        }
        NetworkEvent::BackProp { packet_id, from, via, to, tick } => {
            t.set("type", "BackProp")?;
            t.set("packet_id", packet_id.to_string())?;
            t.set("from", from.to_string())?;
            t.set("via", via.to_string())?;
            t.set("to", to.to_string())?;
            t.set("tick", *tick)?;
        }
        NetworkEvent::Dropped { packet_id, reason, tick } => {
            t.set("type", "Dropped")?;
            t.set("packet_id", packet_id.to_string())?;
            t.set("reason", drop_reason_to_string(reason))?;
            t.set("tick", *tick)?;
        }
    }

    Ok(t)
}

/// Convert a DropReason to a string
fn drop_reason_to_string(reason: &DropReason) -> &'static str {
    match reason {
        DropReason::TtlExpired => "TtlExpired",
        DropReason::NoRoute => "NoRoute",
        DropReason::Duplicate => "Duplicate",
        DropReason::Expired => "Expired",
        DropReason::SenderOffline => "SenderOffline",
    }
}

/// Register event types (for documentation/introspection)
pub fn register(lua: &Lua, indras: &Table) -> Result<()> {
    let events = lua.create_table()?;

    // Event type constants for matching
    events.set("AWAKE", "Awake")?;
    events.set("SLEEP", "Sleep")?;
    events.set("SEND", "Send")?;
    events.set("RELAY", "Relay")?;
    events.set("DELIVERED", "Delivered")?;
    events.set("BACKPROP", "BackProp")?;
    events.set("DROPPED", "Dropped")?;

    // Drop reason constants
    let drop_reasons = lua.create_table()?;
    drop_reasons.set("TTL_EXPIRED", "TtlExpired")?;
    drop_reasons.set("NO_ROUTE", "NoRoute")?;
    drop_reasons.set("DUPLICATE", "Duplicate")?;
    drop_reasons.set("EXPIRED", "Expired")?;
    drop_reasons.set("SENDER_OFFLINE", "SenderOffline")?;
    events.set("DropReason", drop_reasons)?;

    indras.set("events", events)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{PacketId, PeerId};

    #[test]
    fn test_awake_event() {
        let lua = Lua::new();
        let event = NetworkEvent::Awake {
            peer: PeerId('A'),
            tick: 5,
        };
        let t = network_event_to_table(&lua, &event).unwrap();

        let event_type: String = t.get("type").unwrap();
        let peer: String = t.get("peer").unwrap();
        let tick: u64 = t.get("tick").unwrap();

        assert_eq!(event_type, "Awake");
        assert_eq!(peer, "A");
        assert_eq!(tick, 5);
    }

    #[test]
    fn test_delivered_event() {
        let lua = Lua::new();
        let event = NetworkEvent::Delivered {
            packet_id: PacketId {
                source: PeerId('A'),
                sequence: 1,
            },
            to: PeerId('B'),
            tick: 10,
        };
        let t = network_event_to_table(&lua, &event).unwrap();

        let event_type: String = t.get("type").unwrap();
        let packet_id: String = t.get("packet_id").unwrap();

        assert_eq!(event_type, "Delivered");
        assert_eq!(packet_id, "A#1");
    }

    #[test]
    fn test_dropped_event() {
        let lua = Lua::new();
        let event = NetworkEvent::Dropped {
            packet_id: PacketId {
                source: PeerId('A'),
                sequence: 0,
            },
            reason: DropReason::TtlExpired,
            tick: 20,
        };
        let t = network_event_to_table(&lua, &event).unwrap();

        let reason: String = t.get("reason").unwrap();
        assert_eq!(reason, "TtlExpired");
    }

    #[test]
    fn test_event_constants() {
        let lua = Lua::new();
        let indras = lua.create_table().unwrap();
        register(&lua, &indras).unwrap();
        lua.globals().set("indras", indras).unwrap();

        let delivered: String = lua
            .load("return indras.events.DELIVERED")
            .eval()
            .unwrap();
        assert_eq!(delivered, "Delivered");
    }
}
