//! Lua bindings for SimStats
//!
//! Provides read-only access to simulation statistics.

use mlua::{Lua, MetaMethod, Result, Table, UserData, UserDataMethods};

use crate::simulation::SimStats;

/// Lua wrapper for SimStats
#[derive(Debug, Clone, Default)]
pub struct LuaSimStats(pub SimStats);

impl From<SimStats> for LuaSimStats {
    fn from(stats: SimStats) -> Self {
        Self(stats)
    }
}

impl UserData for LuaSimStats {
    fn add_fields<F: mlua::UserDataFields<Self>>(fields: &mut F) {
        // Basic counters
        fields.add_field_method_get("messages_sent", |_, this| Ok(this.0.messages_sent));
        fields.add_field_method_get("messages_delivered", |_, this| Ok(this.0.messages_delivered));
        fields.add_field_method_get("messages_dropped", |_, this| Ok(this.0.messages_dropped));
        fields.add_field_method_get("messages_expired", |_, this| Ok(this.0.messages_expired));

        // Hop tracking
        fields.add_field_method_get("total_hops", |_, this| Ok(this.0.total_hops));
        fields.add_field_method_get("direct_deliveries", |_, this| Ok(this.0.direct_deliveries));
        fields.add_field_method_get("relayed_deliveries", |_, this| Ok(this.0.relayed_deliveries));

        // Back-propagation
        fields.add_field_method_get("backprops_completed", |_, this| Ok(this.0.backprops_completed));
        fields.add_field_method_get("backprops_timed_out", |_, this| Ok(this.0.backprops_timed_out));

        // State transitions
        fields.add_field_method_get("wake_events", |_, this| Ok(this.0.wake_events));
        fields.add_field_method_get("sleep_events", |_, this| Ok(this.0.sleep_events));

        // Latency tracking
        fields.add_field_method_get("total_delivery_latency", |_, this| {
            Ok(this.0.total_delivery_latency)
        });
        fields.add_field_method_get("total_backprop_latency", |_, this| {
            Ok(this.0.total_backprop_latency)
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // delivery_rate() -> float (0.0-1.0)
        methods.add_method("delivery_rate", |_, this, ()| {
            if this.0.messages_sent == 0 {
                return Ok(0.0);
            }
            Ok(this.0.messages_delivered as f64 / this.0.messages_sent as f64)
        });

        // drop_rate() -> float (0.0-1.0)
        methods.add_method("drop_rate", |_, this, ()| {
            if this.0.messages_sent == 0 {
                return Ok(0.0);
            }
            Ok(this.0.messages_dropped as f64 / this.0.messages_sent as f64)
        });

        // average_latency() -> float (ticks)
        methods.add_method("average_latency", |_, this, ()| {
            if this.0.messages_delivered == 0 {
                return Ok(0.0);
            }
            Ok(this.0.total_delivery_latency as f64 / this.0.messages_delivered as f64)
        });

        // average_hops() -> float
        methods.add_method("average_hops", |_, this, ()| {
            if this.0.messages_delivered == 0 {
                return Ok(0.0);
            }
            Ok(this.0.total_hops as f64 / this.0.messages_delivered as f64)
        });

        // backprop_success_rate() -> float
        methods.add_method("backprop_success_rate", |_, this, ()| {
            let total = this.0.backprops_completed + this.0.backprops_timed_out;
            if total == 0 {
                return Ok(1.0); // No backprops = 100% success (vacuous truth)
            }
            Ok(this.0.backprops_completed as f64 / total as f64)
        });

        // average_backprop_latency() -> float (ticks)
        methods.add_method("average_backprop_latency", |_, this, ()| {
            if this.0.backprops_completed == 0 {
                return Ok(0.0);
            }
            Ok(this.0.total_backprop_latency as f64 / this.0.backprops_completed as f64)
        });

        // to_table() -> table with all stats
        methods.add_method("to_table", |lua, this, ()| {
            let t = lua.create_table()?;
            t.set("messages_sent", this.0.messages_sent)?;
            t.set("messages_delivered", this.0.messages_delivered)?;
            t.set("messages_dropped", this.0.messages_dropped)?;
            t.set("messages_expired", this.0.messages_expired)?;
            t.set("total_hops", this.0.total_hops)?;
            t.set("direct_deliveries", this.0.direct_deliveries)?;
            t.set("relayed_deliveries", this.0.relayed_deliveries)?;
            t.set("backprops_completed", this.0.backprops_completed)?;
            t.set("backprops_timed_out", this.0.backprops_timed_out)?;
            t.set("wake_events", this.0.wake_events)?;
            t.set("sleep_events", this.0.sleep_events)?;
            t.set("total_delivery_latency", this.0.total_delivery_latency)?;
            t.set("total_backprop_latency", this.0.total_backprop_latency)?;
            Ok(t)
        });

        // String representation
        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            Ok(format!(
                "SimStats(sent={}, delivered={}, dropped={})",
                this.0.messages_sent, this.0.messages_delivered, this.0.messages_dropped
            ))
        });
    }
}

/// Register stats types (placeholder for consistency)
pub fn register(_lua: &Lua, _indras: &Table) -> Result<()> {
    // SimStats is created by Simulation, not directly constructed
    // This function exists for module consistency
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_lua() -> Lua {
        let lua = Lua::new();
        let indras = lua.create_table().unwrap();
        super::super::types::register(&lua, &indras).unwrap();
        super::super::mesh::register(&lua, &indras).unwrap();
        super::super::events::register(&lua, &indras).unwrap();
        super::super::simulation::register_config(&lua, &indras).unwrap();
        super::super::simulation::register(&lua, &indras).unwrap();
        register(&lua, &indras).unwrap();
        lua.globals().set("indras", indras).unwrap();
        lua
    }

    #[test]
    fn test_stats_fields() {
        let lua = setup_lua();

        let sent: u64 = lua
            .load(r#"
                local mesh = indras.MeshBuilder.new(2):full_mesh()
                local sim = indras.Simulation.new(mesh, indras.SimConfig.manual())
                sim:force_online(indras.PeerId.new('A'))
                sim:force_online(indras.PeerId.new('B'))
                sim:send_message(indras.PeerId.new('A'), indras.PeerId.new('B'), "test")
                return sim.stats.messages_sent
            "#)
            .eval()
            .unwrap();
        assert_eq!(sent, 1);
    }

    #[test]
    fn test_stats_delivery_rate() {
        let lua = setup_lua();

        let rate: f64 = lua
            .load(r#"
                local mesh = indras.MeshBuilder.new(2):full_mesh()
                local sim = indras.Simulation.new(mesh, indras.SimConfig.manual())
                sim:force_online(indras.PeerId.new('A'))
                sim:force_online(indras.PeerId.new('B'))
                sim:send_message(indras.PeerId.new('A'), indras.PeerId.new('B'), "test")
                sim:run_ticks(5)
                return sim.stats:delivery_rate()
            "#)
            .eval()
            .unwrap();
        assert!((rate - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_stats_to_table() {
        let lua = setup_lua();

        let sent: u64 = lua
            .load(r#"
                local mesh = indras.MeshBuilder.new(2):full_mesh()
                local sim = indras.Simulation.new(mesh, indras.SimConfig.manual())
                local t = sim.stats:to_table()
                return t.messages_sent
            "#)
            .eval()
            .unwrap();
        assert_eq!(sent, 0);
    }
}
