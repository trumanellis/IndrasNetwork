//! Lua bindings for Simulation and SimConfig
//!
//! Provides Lua wrappers for the simulation engine and configuration.

use mlua::{FromLua, Lua, MetaMethod, Result, Table, UserData, UserDataMethods, Value};
use std::cell::RefCell;
use std::rc::Rc;

use crate::simulation::{SimConfig, Simulation};

use super::mesh::LuaMesh;
use super::stats::LuaSimStats;
use super::types::LuaPeerId;

/// Lua wrapper for SimConfig
#[derive(Debug, Clone)]
pub struct LuaSimConfig(pub SimConfig);

impl From<SimConfig> for LuaSimConfig {
    fn from(config: SimConfig) -> Self {
        Self(config)
    }
}

impl FromLua for LuaSimConfig {
    fn from_lua(value: Value, _lua: &Lua) -> Result<Self> {
        match value {
            Value::UserData(ud) => ud.borrow::<Self>().map(|v| v.clone()),
            _ => Err(mlua::Error::external("Expected SimConfig userdata")),
        }
    }
}

impl UserData for LuaSimConfig {
    fn add_fields<F: mlua::UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("wake_probability", |_, this| Ok(this.0.wake_probability));
        fields.add_field_method_get("sleep_probability", |_, this| Ok(this.0.sleep_probability));
        fields.add_field_method_get("max_ticks", |_, this| Ok(this.0.max_ticks));
        fields.add_field_method_get("initial_online_probability", |_, this| {
            Ok(this.0.initial_online_probability)
        });
        fields.add_field_method_get("trace_routing", |_, this| Ok(this.0.trace_routing));

        fields.add_field_method_set("wake_probability", |_, this, val: f64| {
            this.0.wake_probability = val;
            Ok(())
        });
        fields.add_field_method_set("sleep_probability", |_, this, val: f64| {
            this.0.sleep_probability = val;
            Ok(())
        });
        fields.add_field_method_set("max_ticks", |_, this, val: u64| {
            this.0.max_ticks = val;
            Ok(())
        });
        fields.add_field_method_set("initial_online_probability", |_, this, val: f64| {
            this.0.initial_online_probability = val;
            Ok(())
        });
        fields.add_field_method_set("trace_routing", |_, this, val: bool| {
            this.0.trace_routing = val;
            Ok(())
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            Ok(format!(
                "SimConfig(wake={}, sleep={}, max_ticks={})",
                this.0.wake_probability, this.0.sleep_probability, this.0.max_ticks
            ))
        });
    }
}

/// Lua wrapper for Simulation (with interior mutability)
pub struct LuaSimulation(pub Rc<RefCell<Simulation>>);

impl LuaSimulation {
    pub fn new(sim: Simulation) -> Self {
        Self(Rc::new(RefCell::new(sim)))
    }
}

impl Clone for LuaSimulation {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl UserData for LuaSimulation {
    fn add_fields<F: mlua::UserDataFields<Self>>(fields: &mut F) {
        // tick - current simulation tick (read-only)
        fields.add_field_method_get("tick", |_, this| {
            Ok(this.0.borrow().tick)
        });

        // stats - simulation statistics
        fields.add_field_method_get("stats", |_, this| {
            let stats = this.0.borrow().stats.clone();
            Ok(LuaSimStats(stats))
        });

        // mesh - the underlying mesh (read-only access)
        fields.add_field_method_get("mesh", |_, this| {
            let mesh = this.0.borrow().mesh.clone();
            Ok(LuaMesh::new(mesh))
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // initialize() - setup initial state
        methods.add_method("initialize", |_, this, ()| {
            this.0.borrow_mut().initialize();
            Ok(())
        });

        // step() - advance one tick
        methods.add_method("step", |_, this, ()| {
            this.0.borrow_mut().step();
            Ok(())
        });

        // run() - run to completion
        methods.add_method("run", |_, this, ()| {
            this.0.borrow_mut().run();
            Ok(())
        });

        // run_ticks(n) - advance n ticks
        methods.add_method("run_ticks", |_, this, n: u64| {
            this.0.borrow_mut().run_ticks(n);
            Ok(())
        });

        // is_online(peer) -> bool
        methods.add_method("is_online", |_, this, peer: LuaPeerId| {
            Ok(this.0.borrow().is_online(peer.0))
        });

        // force_online(peer)
        methods.add_method("force_online", |_, this, peer: LuaPeerId| {
            this.0.borrow_mut().force_online(peer.0);
            Ok(())
        });

        // force_offline(peer)
        methods.add_method("force_offline", |_, this, peer: LuaPeerId| {
            this.0.borrow_mut().force_offline(peer.0);
            Ok(())
        });

        // send_message(from, to, payload)
        methods.add_method("send_message", |_, this, (from, to, payload): (LuaPeerId, LuaPeerId, Value)| {
            let payload_bytes = match payload {
                Value::String(s) => s.as_bytes().to_vec(),
                Value::Table(t) => {
                    // Interpret as byte array
                    let mut bytes = Vec::new();
                    for v in t.sequence_values::<u8>() {
                        bytes.push(v?);
                    }
                    bytes
                }
                _ => return Err(mlua::Error::external("Payload must be string or byte array")),
            };
            this.0.borrow_mut().send_message(from.0, to.0, payload_bytes);
            Ok(())
        });

        // state_summary() -> string
        methods.add_method("state_summary", |_, this, ()| {
            Ok(this.0.borrow().state_summary())
        });

        // online_peers() -> [PeerId]
        methods.add_method("online_peers", |_, this, ()| {
            let sim = this.0.borrow();
            let peers: Vec<LuaPeerId> = sim.mesh.peers.values()
                .filter(|p| p.online)
                .map(|p| LuaPeerId(p.id))
                .collect();
            Ok(peers)
        });

        // offline_peers() -> [PeerId]
        methods.add_method("offline_peers", |_, this, ()| {
            let sim = this.0.borrow();
            let peers: Vec<LuaPeerId> = sim.mesh.peers.values()
                .filter(|p| !p.online)
                .map(|p| LuaPeerId(p.id))
                .collect();
            Ok(peers)
        });

        // event_log() -> [{type, ...}]
        methods.add_method("event_log", |lua, this, ()| {
            let sim = this.0.borrow();
            let events = lua.create_table()?;

            for (i, event) in sim.event_log.iter().enumerate() {
                let event_table = super::events::network_event_to_table(lua, event)?;
                events.set(i + 1, event_table)?;
            }

            Ok(events)
        });

        // String representation
        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            let sim = this.0.borrow();
            Ok(format!(
                "Simulation(tick={}, {} peers, {} events)",
                sim.tick,
                sim.mesh.peer_count(),
                sim.event_log.len()
            ))
        });
    }
}

/// Register SimConfig constructors
pub fn register_config(lua: &Lua, indras: &Table) -> Result<()> {
    let config = lua.create_table()?;

    // SimConfig.default() -> SimConfig with random transitions
    config.set(
        "default",
        lua.create_function(|_, ()| Ok(LuaSimConfig(SimConfig::default())))?,
    )?;

    // SimConfig.manual() -> SimConfig with no random transitions
    config.set(
        "manual",
        lua.create_function(|_, ()| {
            Ok(LuaSimConfig(SimConfig {
                wake_probability: 0.0,
                sleep_probability: 0.0,
                initial_online_probability: 0.0,
                trace_routing: true,
                ..Default::default()
            }))
        })?,
    )?;

    // SimConfig.new(table) -> custom SimConfig
    config.set(
        "new",
        lua.create_function(|_, opts: Option<Table>| {
            let mut cfg = SimConfig::default();

            if let Some(opts) = opts {
                if let Ok(v) = opts.get::<f64>("wake_probability") {
                    cfg.wake_probability = v;
                }
                if let Ok(v) = opts.get::<f64>("sleep_probability") {
                    cfg.sleep_probability = v;
                }
                if let Ok(v) = opts.get::<u64>("max_ticks") {
                    cfg.max_ticks = v;
                }
                if let Ok(v) = opts.get::<f64>("initial_online_probability") {
                    cfg.initial_online_probability = v;
                }
                if let Ok(v) = opts.get::<bool>("trace_routing") {
                    cfg.trace_routing = v;
                }
                if let Ok(v) = opts.get::<u64>("message_timeout") {
                    cfg.message_timeout = Some(v);
                }
                if let Ok(v) = opts.get::<u64>("backprop_timeout") {
                    cfg.backprop_timeout = Some(v);
                }
                if let Ok(v) = opts.get::<u32>("max_sender_retries") {
                    cfg.max_sender_retries = Some(v);
                }
            }

            Ok(LuaSimConfig(cfg))
        })?,
    )?;

    indras.set("SimConfig", config)?;

    Ok(())
}

/// Register Simulation constructor
pub fn register(lua: &Lua, indras: &Table) -> Result<()> {
    let simulation = lua.create_table()?;

    // Simulation.new(mesh, config) -> Simulation
    simulation.set(
        "new",
        lua.create_function(|_, (mesh, config): (LuaMesh, LuaSimConfig)| {
            let mesh_inner = mesh.0.read()
                .map_err(|_| mlua::Error::external("Mesh lock poisoned"))?
                .clone();
            let sim = Simulation::new(mesh_inner, config.0);
            Ok(LuaSimulation::new(sim))
        })?,
    )?;

    indras.set("Simulation", simulation)?;

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
        super::super::stats::register(&lua, &indras).unwrap();
        super::super::events::register(&lua, &indras).unwrap();
        register_config(&lua, &indras).unwrap();
        register(&lua, &indras).unwrap();
        lua.globals().set("indras", indras).unwrap();
        lua
    }

    #[test]
    fn test_sim_config_default() {
        let lua = setup_lua();

        let wake: f64 = lua
            .load(r#"
                local cfg = indras.SimConfig.default()
                return cfg.wake_probability
            "#)
            .eval()
            .unwrap();
        assert!((wake - 0.3).abs() < f64::EPSILON);
    }

    #[test]
    fn test_sim_config_manual() {
        let lua = setup_lua();

        let wake: f64 = lua
            .load(r#"
                local cfg = indras.SimConfig.manual()
                return cfg.wake_probability
            "#)
            .eval()
            .unwrap();
        assert!((wake - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_simulation_creation() {
        let lua = setup_lua();

        let tick: u64 = lua
            .load(r#"
                local mesh = indras.MeshBuilder.new(3):full_mesh()
                local sim = indras.Simulation.new(mesh, indras.SimConfig.manual())
                return sim.tick
            "#)
            .eval()
            .unwrap();
        assert_eq!(tick, 0);
    }

    #[test]
    fn test_simulation_step() {
        let lua = setup_lua();

        let tick: u64 = lua
            .load(r#"
                local mesh = indras.MeshBuilder.new(3):full_mesh()
                local sim = indras.Simulation.new(mesh, indras.SimConfig.manual())
                sim:step()
                sim:step()
                return sim.tick
            "#)
            .eval()
            .unwrap();
        assert_eq!(tick, 2);
    }

    #[test]
    fn test_simulation_force_online() {
        let lua = setup_lua();

        let online: bool = lua
            .load(r#"
                local mesh = indras.MeshBuilder.new(3):full_mesh()
                local sim = indras.Simulation.new(mesh, indras.SimConfig.manual())
                local a = indras.PeerId.new('A')
                sim:force_online(a)
                return sim:is_online(a)
            "#)
            .eval()
            .unwrap();
        assert!(online);
    }

    #[test]
    fn test_simulation_send_message() {
        let lua = setup_lua();

        let delivered: u64 = lua
            .load(r#"
                local mesh = indras.MeshBuilder.new(3):full_mesh()
                local sim = indras.Simulation.new(mesh, indras.SimConfig.manual())

                local a = indras.PeerId.new('A')
                local b = indras.PeerId.new('B')

                sim:force_online(a)
                sim:force_online(b)

                sim:send_message(a, b, "Hello!")
                sim:run_ticks(5)

                return sim.stats.messages_delivered
            "#)
            .eval()
            .unwrap();
        assert_eq!(delivered, 1);
    }

    #[test]
    fn test_simulation_state_summary() {
        let lua = setup_lua();

        let summary: String = lua
            .load(r#"
                local mesh = indras.MeshBuilder.new(3):full_mesh()
                local sim = indras.Simulation.new(mesh, indras.SimConfig.manual())
                sim:force_online(indras.PeerId.new('A'))
                return sim:state_summary()
            "#)
            .eval()
            .unwrap();
        assert!(summary.contains("1 online"));
    }
}
