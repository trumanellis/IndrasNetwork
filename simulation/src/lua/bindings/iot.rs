//! Lua bindings for indras-iot crate
//!
//! Provides Lua wrappers for IoT-optimized types:
//! - DutyCycleManager - power-aware scheduling
//! - CompactMessage - bandwidth-efficient wire format
//! - MemoryTracker - memory allocation limits
//! - BufferPool - buffer management

use mlua::{FromLua, Lua, MetaMethod, Result, Table, UserData, UserDataMethods, Value};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use indras_iot::compact::{CompactMessage, CompactMessageType, Fragmenter};
use indras_iot::duty_cycle::{DutyCycleConfig, DutyCycleManager, PowerState};
use indras_iot::low_memory::{BufferPool, MemoryBudget, MemoryTracker, PooledBuffer};

// =============================================================================
// PowerState binding
// =============================================================================

/// Lua wrapper for PowerState enum
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LuaPowerState(pub PowerState);

impl From<PowerState> for LuaPowerState {
    fn from(state: PowerState) -> Self {
        Self(state)
    }
}

impl FromLua for LuaPowerState {
    fn from_lua(value: Value, _lua: &Lua) -> Result<Self> {
        match value {
            Value::UserData(ud) => ud.borrow::<Self>().map(|v| *v),
            Value::String(s) => {
                let str_val: &str = &s.to_str()?;
                match str_val {
                    "active" | "Active" => Ok(LuaPowerState(PowerState::Active)),
                    "presleep" | "PreSleep" | "pre_sleep" => {
                        Ok(LuaPowerState(PowerState::PreSleep))
                    }
                    "sleeping" | "Sleeping" => Ok(LuaPowerState(PowerState::Sleeping)),
                    "waking" | "Waking" => Ok(LuaPowerState(PowerState::Waking)),
                    other => Err(mlua::Error::external(format!(
                        "Unknown power state: {}",
                        other
                    ))),
                }
            }
            _ => Err(mlua::Error::external(
                "Expected PowerState userdata or string",
            )),
        }
    }
}

impl UserData for LuaPowerState {
    fn add_fields<F: mlua::UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("value", |_, this| {
            Ok(match this.0 {
                PowerState::Active => "active",
                PowerState::PreSleep => "presleep",
                PowerState::Sleeping => "sleeping",
                PowerState::Waking => "waking",
            })
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            Ok(match this.0 {
                PowerState::Active => "active",
                PowerState::PreSleep => "presleep",
                PowerState::Sleeping => "sleeping",
                PowerState::Waking => "waking",
            })
        });

        methods.add_meta_method(MetaMethod::Eq, |_, this, other: LuaPowerState| {
            Ok(this.0 == other.0)
        });
    }
}

// =============================================================================
// DutyCycleConfig binding
// =============================================================================

/// Lua wrapper for DutyCycleConfig
#[derive(Debug, Clone)]
pub struct LuaDutyCycleConfig(pub DutyCycleConfig);

impl FromLua for LuaDutyCycleConfig {
    fn from_lua(value: Value, _lua: &Lua) -> Result<Self> {
        match value {
            Value::UserData(ud) => ud.borrow::<Self>().map(|v| v.clone()),
            _ => Err(mlua::Error::external("Expected DutyCycleConfig userdata")),
        }
    }
}

impl UserData for LuaDutyCycleConfig {
    fn add_fields<F: mlua::UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("active_duration_secs", |_, this| {
            Ok(this.0.active_duration.as_secs_f64())
        });

        fields.add_field_method_get("sleep_duration_secs", |_, this| {
            Ok(this.0.sleep_duration.as_secs_f64())
        });

        fields.add_field_method_get("min_sync_interval_secs", |_, this| {
            Ok(this.0.min_sync_interval.as_secs_f64())
        });

        fields.add_field_method_get("max_pending_before_wake", |_, this| {
            Ok(this.0.max_pending_before_wake)
        });

        fields.add_field_method_get("low_battery_threshold", |_, this| {
            Ok(this.0.low_battery_threshold)
        });

        fields.add_field_method_get("duty_percentage", |_, this| {
            Ok(this.0.duty_percentage())
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            Ok(format!(
                "DutyCycleConfig(active={}s, sleep={}s, duty={:.1}%)",
                this.0.active_duration.as_secs(),
                this.0.sleep_duration.as_secs(),
                this.0.duty_percentage()
            ))
        });
    }
}

// =============================================================================
// DutyCycleManager binding
// =============================================================================

/// Lua wrapper for DutyCycleManager (thread-safe with interior mutability)
#[derive(Clone)]
pub struct LuaDutyCycleManager(pub Arc<Mutex<DutyCycleManager>>);

impl LuaDutyCycleManager {
    pub fn new(manager: DutyCycleManager) -> Self {
        Self(Arc::new(Mutex::new(manager)))
    }
}

impl FromLua for LuaDutyCycleManager {
    fn from_lua(value: Value, _lua: &Lua) -> Result<Self> {
        match value {
            Value::UserData(ud) => ud.borrow::<Self>().map(|v| v.clone()),
            _ => Err(mlua::Error::external("Expected DutyCycleManager userdata")),
        }
    }
}

impl UserData for LuaDutyCycleManager {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // state() -> PowerState
        methods.add_method("state", |_, this, ()| {
            let manager = this
                .0
                .lock()
                .map_err(|_| mlua::Error::external("DutyCycleManager lock poisoned"))?;
            Ok(LuaPowerState(manager.state()))
        });

        // config() -> DutyCycleConfig
        methods.add_method("config", |_, this, ()| {
            let manager = this
                .0
                .lock()
                .map_err(|_| mlua::Error::external("DutyCycleManager lock poisoned"))?;
            Ok(LuaDutyCycleConfig(manager.config().clone()))
        });

        // battery_level() -> number
        methods.add_method("battery_level", |_, this, ()| {
            let manager = this
                .0
                .lock()
                .map_err(|_| mlua::Error::external("DutyCycleManager lock poisoned"))?;
            Ok(manager.battery_level())
        });

        // set_battery_level(level)
        methods.add_method("set_battery_level", |_, this, level: f32| {
            let mut manager = this
                .0
                .lock()
                .map_err(|_| mlua::Error::external("DutyCycleManager lock poisoned"))?;
            manager.set_battery_level(level);
            Ok(())
        });

        // should_allow_operation(is_urgent) -> bool
        methods.add_method("should_allow_operation", |_, this, is_urgent: bool| {
            let manager = this
                .0
                .lock()
                .map_err(|_| mlua::Error::external("DutyCycleManager lock poisoned"))?;
            Ok(manager.should_allow_operation(is_urgent))
        });

        // should_sync() -> bool
        methods.add_method("should_sync", |_, this, ()| {
            let manager = this
                .0
                .lock()
                .map_err(|_| mlua::Error::external("DutyCycleManager lock poisoned"))?;
            Ok(manager.should_sync())
        });

        // record_sync()
        methods.add_method("record_sync", |_, this, ()| {
            let mut manager = this
                .0
                .lock()
                .map_err(|_| mlua::Error::external("DutyCycleManager lock poisoned"))?;
            manager.record_sync();
            Ok(())
        });

        // add_pending() -> bool (returns true on success, false on error)
        methods.add_method("add_pending", |_, this, ()| {
            let mut manager = this
                .0
                .lock()
                .map_err(|_| mlua::Error::external("DutyCycleManager lock poisoned"))?;
            match manager.add_pending() {
                Ok(()) => Ok(true),
                Err(_) => Ok(false),
            }
        });

        // complete_pending()
        methods.add_method("complete_pending", |_, this, ()| {
            let mut manager = this
                .0
                .lock()
                .map_err(|_| mlua::Error::external("DutyCycleManager lock poisoned"))?;
            manager.complete_pending();
            Ok(())
        });

        // clear_pending()
        methods.add_method("clear_pending", |_, this, ()| {
            let mut manager = this
                .0
                .lock()
                .map_err(|_| mlua::Error::external("DutyCycleManager lock poisoned"))?;
            manager.clear_pending();
            Ok(())
        });

        // pending_count() -> number
        methods.add_method("pending_count", |_, this, ()| {
            let manager = this
                .0
                .lock()
                .map_err(|_| mlua::Error::external("DutyCycleManager lock poisoned"))?;
            Ok(manager.pending_count())
        });

        // tick() -> PowerState
        methods.add_method("tick", |_, this, ()| {
            let mut manager = this
                .0
                .lock()
                .map_err(|_| mlua::Error::external("DutyCycleManager lock poisoned"))?;
            Ok(LuaPowerState(manager.tick()))
        });

        // wake()
        methods.add_method("wake", |_, this, ()| {
            let mut manager = this
                .0
                .lock()
                .map_err(|_| mlua::Error::external("DutyCycleManager lock poisoned"))?;
            manager.wake();
            Ok(())
        });

        // sleep()
        methods.add_method("sleep", |_, this, ()| {
            let mut manager = this
                .0
                .lock()
                .map_err(|_| mlua::Error::external("DutyCycleManager lock poisoned"))?;
            manager.sleep();
            Ok(())
        });

        // time_until_transition() -> number (seconds)
        methods.add_method("time_until_transition", |_, this, ()| {
            let manager = this
                .0
                .lock()
                .map_err(|_| mlua::Error::external("DutyCycleManager lock poisoned"))?;
            Ok(manager.time_until_transition().as_secs_f64())
        });

        // String representation
        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            let manager = this
                .0
                .lock()
                .map_err(|_| mlua::Error::external("DutyCycleManager lock poisoned"))?;
            Ok(format!(
                "DutyCycleManager(state={:?}, battery={:.0}%, pending={})",
                manager.state(),
                manager.battery_level() * 100.0,
                manager.pending_count()
            ))
        });
    }
}

// =============================================================================
// CompactMessageType binding
// =============================================================================

/// Lua wrapper for CompactMessageType enum
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LuaCompactMessageType(pub CompactMessageType);

impl From<CompactMessageType> for LuaCompactMessageType {
    fn from(msg_type: CompactMessageType) -> Self {
        Self(msg_type)
    }
}

impl FromLua for LuaCompactMessageType {
    fn from_lua(value: Value, _lua: &Lua) -> Result<Self> {
        match value {
            Value::UserData(ud) => ud.borrow::<Self>().map(|v| *v),
            Value::String(s) => {
                let str_val: &str = &s.to_str()?;
                match str_val {
                    "ping" | "Ping" => Ok(LuaCompactMessageType(CompactMessageType::Ping)),
                    "pong" | "Pong" => Ok(LuaCompactMessageType(CompactMessageType::Pong)),
                    "data" | "Data" => Ok(LuaCompactMessageType(CompactMessageType::Data)),
                    "ack" | "Ack" => Ok(LuaCompactMessageType(CompactMessageType::Ack)),
                    "sync_request" | "SyncRequest" => {
                        Ok(LuaCompactMessageType(CompactMessageType::SyncRequest))
                    }
                    "sync_response" | "SyncResponse" => {
                        Ok(LuaCompactMessageType(CompactMessageType::SyncResponse))
                    }
                    "presence" | "Presence" => {
                        Ok(LuaCompactMessageType(CompactMessageType::Presence))
                    }
                    other => Err(mlua::Error::external(format!(
                        "Unknown message type: {}",
                        other
                    ))),
                }
            }
            _ => Err(mlua::Error::external(
                "Expected CompactMessageType userdata or string",
            )),
        }
    }
}

impl UserData for LuaCompactMessageType {
    fn add_fields<F: mlua::UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("value", |_, this| {
            Ok(match this.0 {
                CompactMessageType::Ping => "ping",
                CompactMessageType::Pong => "pong",
                CompactMessageType::Data => "data",
                CompactMessageType::Ack => "ack",
                CompactMessageType::SyncRequest => "sync_request",
                CompactMessageType::SyncResponse => "sync_response",
                CompactMessageType::Presence => "presence",
            })
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            Ok(match this.0 {
                CompactMessageType::Ping => "ping",
                CompactMessageType::Pong => "pong",
                CompactMessageType::Data => "data",
                CompactMessageType::Ack => "ack",
                CompactMessageType::SyncRequest => "sync_request",
                CompactMessageType::SyncResponse => "sync_response",
                CompactMessageType::Presence => "presence",
            })
        });

        methods.add_meta_method(
            MetaMethod::Eq,
            |_, this, other: LuaCompactMessageType| Ok(this.0 == other.0),
        );
    }
}

// =============================================================================
// CompactMessage binding
// =============================================================================

/// Lua wrapper for CompactMessage
#[derive(Debug, Clone)]
pub struct LuaCompactMessage(pub CompactMessage);

impl From<CompactMessage> for LuaCompactMessage {
    fn from(msg: CompactMessage) -> Self {
        Self(msg)
    }
}

impl FromLua for LuaCompactMessage {
    fn from_lua(value: Value, _lua: &Lua) -> Result<Self> {
        match value {
            Value::UserData(ud) => ud.borrow::<Self>().map(|v| v.clone()),
            _ => Err(mlua::Error::external("Expected CompactMessage userdata")),
        }
    }
}

impl UserData for LuaCompactMessage {
    fn add_fields<F: mlua::UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("msg_type", |_, this| {
            Ok(LuaCompactMessageType(this.0.msg_type))
        });

        fields.add_field_method_get("flags", |_, this| Ok(this.0.flags));

        fields.add_field_method_get("sequence", |_, this| Ok(this.0.sequence));

        fields.add_field_method_get("payload", |_, this| Ok(this.0.payload.clone()));

        fields.add_field_method_get("payload_string", |_, this| {
            Ok(String::from_utf8_lossy(&this.0.payload).to_string())
        });

        fields.add_field_method_get("encoded_size", |_, this| Ok(this.0.encoded_size()));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // ack_requested() -> bool
        methods.add_method("ack_requested", |_, this, ()| Ok(this.0.ack_requested()));

        // is_fragmented() -> bool
        methods.add_method("is_fragmented", |_, this, ()| Ok(this.0.is_fragmented()));

        // is_last_fragment() -> bool
        methods.add_method("is_last_fragment", |_, this, ()| {
            Ok(this.0.is_last_fragment())
        });

        // fragment_index() -> number
        methods.add_method("fragment_index", |_, this, ()| Ok(this.0.fragment_index()));

        // original_sequence() -> number
        methods.add_method("original_sequence", |_, this, ()| {
            Ok(this.0.original_sequence())
        });

        // with_ack_requested() -> CompactMessage
        methods.add_method("with_ack_requested", |_, this, ()| {
            Ok(LuaCompactMessage(this.0.clone().with_ack_requested()))
        });

        // with_sequence(seq) -> CompactMessage
        methods.add_method("with_sequence", |_, this, seq: u32| {
            Ok(LuaCompactMessage(this.0.clone().with_sequence(seq)))
        });

        // encode() -> bytes (as Lua string)
        methods.add_method("encode", |lua, this, ()| {
            let encoded = this
                .0
                .encode()
                .map_err(|e| mlua::Error::external(format!("Encode error: {}", e)))?;
            lua.create_string(&encoded)
        });

        // String representation
        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            Ok(format!(
                "CompactMessage(type={:?}, seq={}, payload_len={})",
                this.0.msg_type,
                this.0.sequence,
                this.0.payload.len()
            ))
        });
    }
}

// =============================================================================
// Fragmenter binding
// =============================================================================

/// Lua wrapper for Fragmenter
pub struct LuaFragmenter(pub Fragmenter);

impl UserData for LuaFragmenter {
    fn add_fields<F: mlua::UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("max_fragment_size", |_, this| {
            Ok(this.0.max_fragment_size())
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // fragment(message) -> [CompactMessage]
        methods.add_method("fragment", |_, this, msg: LuaCompactMessage| {
            let fragments: Vec<LuaCompactMessage> =
                this.0.fragment(&msg.0).into_iter().map(LuaCompactMessage).collect();
            Ok(fragments)
        });

        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            Ok(format!("Fragmenter(max_size={})", this.0.max_fragment_size()))
        });
    }
}

// =============================================================================
// MemoryBudget binding
// =============================================================================

/// Lua wrapper for MemoryBudget
#[derive(Debug, Clone)]
pub struct LuaMemoryBudget(pub MemoryBudget);

impl FromLua for LuaMemoryBudget {
    fn from_lua(value: Value, _lua: &Lua) -> Result<Self> {
        match value {
            Value::UserData(ud) => ud.borrow::<Self>().map(|v| v.clone()),
            _ => Err(mlua::Error::external("Expected MemoryBudget userdata")),
        }
    }
}

impl UserData for LuaMemoryBudget {
    fn add_fields<F: mlua::UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("max_heap_bytes", |_, this| Ok(this.0.max_heap_bytes));
        fields.add_field_method_get("max_message_size", |_, this| Ok(this.0.max_message_size));
        fields.add_field_method_get("max_connections", |_, this| Ok(this.0.max_connections));
        fields.add_field_method_get("max_pending_ops", |_, this| Ok(this.0.max_pending_ops));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            Ok(format!(
                "MemoryBudget(heap={}KB, msg={}B, conns={}, ops={})",
                this.0.max_heap_bytes / 1024,
                this.0.max_message_size,
                this.0.max_connections,
                this.0.max_pending_ops
            ))
        });
    }
}

// =============================================================================
// MemoryTracker binding
// =============================================================================

/// Lua wrapper for MemoryTracker (thread-safe via Arc)
#[derive(Clone)]
pub struct LuaMemoryTracker(pub Arc<MemoryTracker>);

impl LuaMemoryTracker {
    pub fn new(tracker: MemoryTracker) -> Self {
        Self(Arc::new(tracker))
    }
}

impl FromLua for LuaMemoryTracker {
    fn from_lua(value: Value, _lua: &Lua) -> Result<Self> {
        match value {
            Value::UserData(ud) => ud.borrow::<Self>().map(|v| v.clone()),
            _ => Err(mlua::Error::external("Expected MemoryTracker userdata")),
        }
    }
}

impl UserData for LuaMemoryTracker {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // allocated_bytes() -> number
        methods.add_method("allocated_bytes", |_, this, ()| {
            Ok(this.0.allocated_bytes())
        });

        // available_bytes() -> number
        methods.add_method("available_bytes", |_, this, ()| {
            Ok(this.0.available_bytes())
        });

        // connection_count() -> number
        methods.add_method("connection_count", |_, this, ()| {
            Ok(this.0.connection_count())
        });

        // pending_ops_count() -> number
        methods.add_method("pending_ops_count", |_, this, ()| {
            Ok(this.0.pending_ops_count())
        });

        // budget() -> MemoryBudget
        methods.add_method("budget", |_, this, ()| {
            Ok(LuaMemoryBudget(this.0.budget().clone()))
        });

        // check_message_size(size) -> bool
        methods.add_method("check_message_size", |_, this, size: usize| {
            Ok(this.0.check_message_size(size).is_ok())
        });

        // try_allocate(bytes) -> bool, allocated_bytes
        // Returns (success, current_allocated) for Lua-friendly interface
        methods.add_method("try_allocate", |_, this, bytes: usize| {
            match this.0.try_allocate(bytes) {
                Ok(guard) => {
                    // Note: The guard is dropped here, releasing the memory.
                    // For real usage, we'd need to return a handle.
                    // For simulation purposes, just test if allocation would succeed.
                    let allocated = this.0.allocated_bytes();
                    drop(guard);
                    Ok((true, allocated))
                }
                Err(_) => Ok((false, this.0.allocated_bytes())),
            }
        });

        // can_allocate(bytes) -> bool
        // Tests if allocation would succeed without actually allocating
        methods.add_method("can_allocate", |_, this, bytes: usize| {
            Ok(this.0.available_bytes() >= bytes)
        });

        // try_add_connection() -> bool
        methods.add_method("try_add_connection", |_, this, ()| {
            match this.0.try_add_connection() {
                Ok(guard) => {
                    // Connection guard dropped immediately for testing
                    drop(guard);
                    Ok(true)
                }
                Err(_) => Ok(false),
            }
        });

        // can_add_connection() -> bool
        methods.add_method("can_add_connection", |_, this, ()| {
            Ok(this.0.connection_count() < this.0.budget().max_connections)
        });

        // try_queue_op() -> bool
        methods.add_method("try_queue_op", |_, this, ()| {
            match this.0.try_queue_op() {
                Ok(guard) => {
                    drop(guard);
                    Ok(true)
                }
                Err(_) => Ok(false),
            }
        });

        // can_queue_op() -> bool
        methods.add_method("can_queue_op", |_, this, ()| {
            Ok(this.0.pending_ops_count() < this.0.budget().max_pending_ops)
        });

        // String representation
        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            Ok(format!(
                "MemoryTracker(allocated={}B, available={}B, conns={}, ops={})",
                this.0.allocated_bytes(),
                this.0.available_bytes(),
                this.0.connection_count(),
                this.0.pending_ops_count()
            ))
        });
    }
}

// =============================================================================
// BufferPool binding
// =============================================================================

/// Lua wrapper for BufferPool (with interior mutability)
#[derive(Clone)]
pub struct LuaBufferPool(pub Arc<Mutex<BufferPool>>);

impl LuaBufferPool {
    pub fn new(pool: BufferPool) -> Self {
        Self(Arc::new(Mutex::new(pool)))
    }
}

impl FromLua for LuaBufferPool {
    fn from_lua(value: Value, _lua: &Lua) -> Result<Self> {
        match value {
            Value::UserData(ud) => ud.borrow::<Self>().map(|v| v.clone()),
            _ => Err(mlua::Error::external("Expected BufferPool userdata")),
        }
    }
}

/// Lua wrapper for PooledBuffer
pub struct LuaPooledBuffer {
    buffer: Option<PooledBuffer>,
}

impl UserData for LuaPooledBuffer {
    fn add_fields<F: mlua::UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("index", |_, this| {
            this.buffer
                .as_ref()
                .map(|b| b.index())
                .ok_or_else(|| mlua::Error::external("Buffer already released"))
        });

        fields.add_field_method_get("size", |_, this| {
            this.buffer
                .as_ref()
                .map(|b| b.buffer.len())
                .ok_or_else(|| mlua::Error::external("Buffer already released"))
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // get_data() -> string (as bytes)
        methods.add_method("get_data", |lua, this, ()| {
            let buffer = this
                .buffer
                .as_ref()
                .ok_or_else(|| mlua::Error::external("Buffer already released"))?;
            lua.create_string(&buffer.buffer)
        });

        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            match &this.buffer {
                Some(b) => Ok(format!("PooledBuffer(index={}, size={})", b.index(), b.buffer.len())),
                None => Ok("PooledBuffer(released)".to_string()),
            }
        });
    }
}

impl UserData for LuaBufferPool {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // buffer_size() -> number
        methods.add_method("buffer_size", |_, this, ()| {
            let pool = this
                .0
                .lock()
                .map_err(|_| mlua::Error::external("BufferPool lock poisoned"))?;
            Ok(pool.buffer_size())
        });

        // capacity() -> number
        methods.add_method("capacity", |_, this, ()| {
            let pool = this
                .0
                .lock()
                .map_err(|_| mlua::Error::external("BufferPool lock poisoned"))?;
            Ok(pool.capacity())
        });

        // available() -> number
        methods.add_method("available", |_, this, ()| {
            let pool = this
                .0
                .lock()
                .map_err(|_| mlua::Error::external("BufferPool lock poisoned"))?;
            Ok(pool.available())
        });

        // in_use() -> number
        methods.add_method("in_use", |_, this, ()| {
            let pool = this
                .0
                .lock()
                .map_err(|_| mlua::Error::external("BufferPool lock poisoned"))?;
            Ok(pool.in_use())
        });

        // try_acquire() -> PooledBuffer or nil
        methods.add_method("try_acquire", |_, this, ()| {
            let mut pool = this
                .0
                .lock()
                .map_err(|_| mlua::Error::external("BufferPool lock poisoned"))?;
            match pool.try_acquire() {
                Some(buffer) => Ok(Some(LuaPooledBuffer {
                    buffer: Some(buffer),
                })),
                None => Ok(None),
            }
        });

        // release(buffer) - releases a buffer back to the pool
        methods.add_method(
            "release",
            |_, this, mut buffer: mlua::UserDataRefMut<LuaPooledBuffer>| {
                let pooled = buffer
                    .buffer
                    .take()
                    .ok_or_else(|| mlua::Error::external("Buffer already released"))?;
                let mut pool = this
                    .0
                    .lock()
                    .map_err(|_| mlua::Error::external("BufferPool lock poisoned"))?;
                pool.release(pooled);
                Ok(())
            },
        );

        // String representation
        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            let pool = this
                .0
                .lock()
                .map_err(|_| mlua::Error::external("BufferPool lock poisoned"))?;
            Ok(format!(
                "BufferPool(capacity={}, available={}, buffer_size={})",
                pool.capacity(),
                pool.available(),
                pool.buffer_size()
            ))
        });
    }
}

// =============================================================================
// Registration
// =============================================================================

/// Register IoT constructors with the indras table
pub fn register(lua: &Lua, indras: &Table) -> Result<()> {
    // Create iot namespace table
    let iot = lua.create_table()?;

    // =================================
    // PowerState constants
    // =================================
    let power_state = lua.create_table()?;
    power_state.set(
        "active",
        lua.create_function(|_, ()| Ok(LuaPowerState(PowerState::Active)))?,
    )?;
    power_state.set(
        "presleep",
        lua.create_function(|_, ()| Ok(LuaPowerState(PowerState::PreSleep)))?,
    )?;
    power_state.set(
        "sleeping",
        lua.create_function(|_, ()| Ok(LuaPowerState(PowerState::Sleeping)))?,
    )?;
    power_state.set(
        "waking",
        lua.create_function(|_, ()| Ok(LuaPowerState(PowerState::Waking)))?,
    )?;
    iot.set("PowerState", power_state)?;

    // =================================
    // DutyCycleConfig constructors
    // =================================
    let duty_cycle_config = lua.create_table()?;

    duty_cycle_config.set(
        "default",
        lua.create_function(|_, ()| Ok(LuaDutyCycleConfig(DutyCycleConfig::default())))?,
    )?;

    duty_cycle_config.set(
        "low_power",
        lua.create_function(|_, ()| Ok(LuaDutyCycleConfig(DutyCycleConfig::low_power())))?,
    )?;

    duty_cycle_config.set(
        "balanced",
        lua.create_function(|_, ()| Ok(LuaDutyCycleConfig(DutyCycleConfig::balanced())))?,
    )?;

    duty_cycle_config.set(
        "responsive",
        lua.create_function(|_, ()| Ok(LuaDutyCycleConfig(DutyCycleConfig::responsive())))?,
    )?;

    // DutyCycleConfig.new(active_secs, sleep_secs, min_sync_secs, max_pending, low_battery)
    duty_cycle_config.set(
        "new",
        lua.create_function(
            |_,
             (active_secs, sleep_secs, min_sync_secs, max_pending, low_battery): (
                f64,
                f64,
                f64,
                usize,
                f32,
            )| {
                Ok(LuaDutyCycleConfig(DutyCycleConfig {
                    active_duration: Duration::from_secs_f64(active_secs),
                    sleep_duration: Duration::from_secs_f64(sleep_secs),
                    min_sync_interval: Duration::from_secs_f64(min_sync_secs),
                    max_pending_before_wake: max_pending,
                    low_battery_threshold: low_battery,
                }))
            },
        )?,
    )?;

    iot.set("DutyCycleConfig", duty_cycle_config)?;

    // =================================
    // DutyCycleManager constructor
    // =================================
    let duty_cycle_manager = lua.create_table()?;

    duty_cycle_manager.set(
        "new",
        lua.create_function(|_, config: LuaDutyCycleConfig| {
            Ok(LuaDutyCycleManager::new(DutyCycleManager::new(config.0)))
        })?,
    )?;

    // Convenience: create with default config
    duty_cycle_manager.set(
        "default",
        lua.create_function(|_, ()| {
            Ok(LuaDutyCycleManager::new(DutyCycleManager::new(
                DutyCycleConfig::default(),
            )))
        })?,
    )?;

    iot.set("DutyCycleManager", duty_cycle_manager)?;

    // =================================
    // CompactMessageType constants
    // =================================
    let msg_type = lua.create_table()?;
    msg_type.set(
        "ping",
        lua.create_function(|_, ()| Ok(LuaCompactMessageType(CompactMessageType::Ping)))?,
    )?;
    msg_type.set(
        "pong",
        lua.create_function(|_, ()| Ok(LuaCompactMessageType(CompactMessageType::Pong)))?,
    )?;
    msg_type.set(
        "data",
        lua.create_function(|_, ()| Ok(LuaCompactMessageType(CompactMessageType::Data)))?,
    )?;
    msg_type.set(
        "ack",
        lua.create_function(|_, ()| Ok(LuaCompactMessageType(CompactMessageType::Ack)))?,
    )?;
    msg_type.set(
        "sync_request",
        lua.create_function(|_, ()| Ok(LuaCompactMessageType(CompactMessageType::SyncRequest)))?,
    )?;
    msg_type.set(
        "sync_response",
        lua.create_function(|_, ()| Ok(LuaCompactMessageType(CompactMessageType::SyncResponse)))?,
    )?;
    msg_type.set(
        "presence",
        lua.create_function(|_, ()| Ok(LuaCompactMessageType(CompactMessageType::Presence)))?,
    )?;
    iot.set("MessageType", msg_type)?;

    // =================================
    // CompactMessage constructors
    // =================================
    let compact_message = lua.create_table()?;

    // CompactMessage.new(type, payload) -> CompactMessage
    compact_message.set(
        "new",
        lua.create_function(
            |_, (msg_type, payload): (LuaCompactMessageType, mlua::String)| {
                let payload_bytes = payload.as_bytes().to_vec();
                Ok(LuaCompactMessage(CompactMessage::new(
                    msg_type.0,
                    payload_bytes,
                )))
            },
        )?,
    )?;

    // CompactMessage.ping() -> CompactMessage
    compact_message.set(
        "ping",
        lua.create_function(|_, ()| Ok(LuaCompactMessage(CompactMessage::ping())))?,
    )?;

    // CompactMessage.pong() -> CompactMessage
    compact_message.set(
        "pong",
        lua.create_function(|_, ()| Ok(LuaCompactMessage(CompactMessage::pong())))?,
    )?;

    // CompactMessage.data(payload) -> CompactMessage
    compact_message.set(
        "data",
        lua.create_function(|_, payload: mlua::String| {
            Ok(LuaCompactMessage(CompactMessage::data(
                payload.as_bytes().to_vec(),
            )))
        })?,
    )?;

    // CompactMessage.ack(sequence) -> CompactMessage
    compact_message.set(
        "ack",
        lua.create_function(|_, seq: u32| Ok(LuaCompactMessage(CompactMessage::ack(seq))))?,
    )?;

    // CompactMessage.decode(bytes) -> CompactMessage
    compact_message.set(
        "decode",
        lua.create_function(|_, data: mlua::String| {
            let bytes = data.as_bytes();
            let msg = CompactMessage::decode(&bytes)
                .map_err(|e| mlua::Error::external(format!("Decode error: {}", e)))?;
            Ok(LuaCompactMessage(msg))
        })?,
    )?;

    iot.set("CompactMessage", compact_message)?;

    // =================================
    // Fragmenter constructor
    // =================================
    let fragmenter = lua.create_table()?;

    fragmenter.set(
        "new",
        lua.create_function(|_, max_fragment_size: usize| {
            if max_fragment_size == 0 {
                return Err(mlua::Error::external(
                    "max_fragment_size must be positive",
                ));
            }
            Ok(LuaFragmenter(Fragmenter::new(max_fragment_size)))
        })?,
    )?;

    iot.set("Fragmenter", fragmenter)?;

    // =================================
    // MemoryBudget constructors
    // =================================
    let memory_budget = lua.create_table()?;

    memory_budget.set(
        "default",
        lua.create_function(|_, ()| Ok(LuaMemoryBudget(MemoryBudget::default())))?,
    )?;

    memory_budget.set(
        "minimal",
        lua.create_function(|_, ()| Ok(LuaMemoryBudget(MemoryBudget::minimal())))?,
    )?;

    memory_budget.set(
        "moderate",
        lua.create_function(|_, ()| Ok(LuaMemoryBudget(MemoryBudget::moderate())))?,
    )?;

    // MemoryBudget.new(max_heap, max_msg, max_conns, max_ops) -> MemoryBudget
    memory_budget.set(
        "new",
        lua.create_function(
            |_, (max_heap, max_msg, max_conns, max_ops): (usize, usize, usize, usize)| {
                Ok(LuaMemoryBudget(MemoryBudget {
                    max_heap_bytes: max_heap,
                    max_message_size: max_msg,
                    max_connections: max_conns,
                    max_pending_ops: max_ops,
                }))
            },
        )?,
    )?;

    iot.set("MemoryBudget", memory_budget)?;

    // =================================
    // MemoryTracker constructor
    // =================================
    let memory_tracker = lua.create_table()?;

    memory_tracker.set(
        "new",
        lua.create_function(|_, budget: LuaMemoryBudget| {
            Ok(LuaMemoryTracker::new(MemoryTracker::new(budget.0)))
        })?,
    )?;

    // Convenience: create with default budget
    memory_tracker.set(
        "default",
        lua.create_function(|_, ()| {
            Ok(LuaMemoryTracker::new(MemoryTracker::new(
                MemoryBudget::default(),
            )))
        })?,
    )?;

    iot.set("MemoryTracker", memory_tracker)?;

    // =================================
    // BufferPool constructor
    // =================================
    let buffer_pool = lua.create_table()?;

    buffer_pool.set(
        "new",
        lua.create_function(|_, (count, buffer_size): (usize, usize)| {
            if buffer_size == 0 {
                return Err(mlua::Error::external("buffer_size must be positive"));
            }
            Ok(LuaBufferPool::new(BufferPool::new(count, buffer_size)))
        })?,
    )?;

    iot.set("BufferPool", buffer_pool)?;

    // Set the iot namespace on indras
    indras.set("iot", iot)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_lua() -> Lua {
        let lua = Lua::new();
        let indras = lua.create_table().unwrap();
        register(&lua, &indras).unwrap();
        lua.globals().set("indras", indras).unwrap();
        lua
    }

    #[test]
    fn test_power_state_constants() {
        let lua = setup_lua();

        let result: String = lua
            .load(
                r#"
                local state = indras.iot.PowerState.active()
                return tostring(state)
            "#,
            )
            .eval()
            .unwrap();
        assert_eq!(result, "active");
    }

    #[test]
    fn test_duty_cycle_config_presets() {
        let lua = setup_lua();

        let result: f64 = lua
            .load(
                r#"
                local config = indras.iot.DutyCycleConfig.responsive()
                return config.duty_percentage
            "#,
            )
            .eval()
            .unwrap();
        assert!((result - 50.0).abs() < 1.0);
    }

    #[test]
    fn test_duty_cycle_manager() {
        let lua = setup_lua();

        let result: String = lua
            .load(
                r#"
                local manager = indras.iot.DutyCycleManager.default()
                return tostring(manager:state())
            "#,
            )
            .eval()
            .unwrap();
        assert_eq!(result, "active");
    }

    #[test]
    fn test_duty_cycle_manager_operations() {
        let lua = setup_lua();

        let result: bool = lua
            .load(
                r#"
                local manager = indras.iot.DutyCycleManager.default()
                manager:set_battery_level(0.5)
                return manager:battery_level() == 0.5
            "#,
            )
            .eval()
            .unwrap();
        assert!(result);
    }

    #[test]
    fn test_compact_message_ping() {
        let lua = setup_lua();

        let result: usize = lua
            .load(
                r#"
                local msg = indras.iot.CompactMessage.ping()
                return msg.encoded_size
            "#,
            )
            .eval()
            .unwrap();
        assert!(result < 10); // Ping should be very compact
    }

    #[test]
    fn test_compact_message_data() {
        let lua = setup_lua();

        let result: String = lua
            .load(
                r#"
                local msg = indras.iot.CompactMessage.data("hello")
                return msg.payload_string
            "#,
            )
            .eval()
            .unwrap();
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_compact_message_encode_decode() {
        let lua = setup_lua();

        let result: (String, u32) = lua
            .load(
                r#"
                local msg = indras.iot.CompactMessage.data("test"):with_sequence(42)
                local encoded = msg:encode()
                local decoded = indras.iot.CompactMessage.decode(encoded)
                return decoded.payload_string, decoded.sequence
            "#,
            )
            .eval()
            .unwrap();
        assert_eq!(result.0, "test");
        assert_eq!(result.1, 42);
    }

    #[test]
    fn test_fragmenter() {
        let lua = setup_lua();

        let count: i32 = lua
            .load(
                r#"
                local fragmenter = indras.iot.Fragmenter.new(5)
                local msg = indras.iot.CompactMessage.data("hello world!")
                local fragments = fragmenter:fragment(msg)
                return #fragments
            "#,
            )
            .eval()
            .unwrap();
        assert!(count > 1);
    }

    #[test]
    fn test_memory_budget() {
        let lua = setup_lua();

        let result: usize = lua
            .load(
                r#"
                local budget = indras.iot.MemoryBudget.minimal()
                return budget.max_heap_bytes
            "#,
            )
            .eval()
            .unwrap();
        assert_eq!(result, 16 * 1024); // 16KB for minimal
    }

    #[test]
    fn test_memory_tracker() {
        let lua = setup_lua();

        let result: bool = lua
            .load(
                r#"
                local tracker = indras.iot.MemoryTracker.default()
                return tracker:can_allocate(1024)
            "#,
            )
            .eval()
            .unwrap();
        assert!(result);
    }

    #[test]
    fn test_memory_tracker_operations() {
        let lua = setup_lua();

        let result: (bool, usize) = lua
            .load(
                r#"
                local budget = indras.iot.MemoryBudget.new(1024, 256, 2, 4)
                local tracker = indras.iot.MemoryTracker.new(budget)
                local success, allocated = tracker:try_allocate(512)
                return success, tracker:available_bytes()
            "#,
            )
            .eval()
            .unwrap();
        // After allocation and immediate release, available should be back to max
        assert!(result.0);
        assert_eq!(result.1, 1024);
    }

    #[test]
    fn test_buffer_pool() {
        let lua = setup_lua();

        let result: (usize, usize) = lua
            .load(
                r#"
                local pool = indras.iot.BufferPool.new(4, 256)
                return pool:capacity(), pool:available()
            "#,
            )
            .eval()
            .unwrap();
        assert_eq!(result.0, 4);
        assert_eq!(result.1, 4);
    }

    #[test]
    fn test_buffer_pool_acquire_release() {
        let lua = setup_lua();

        let result: (usize, usize, usize) = lua
            .load(
                r#"
                local pool = indras.iot.BufferPool.new(2, 64)
                local buf = pool:try_acquire()
                local in_use_after_acquire = pool:in_use()
                pool:release(buf)
                local in_use_after_release = pool:in_use()
                return pool:capacity(), in_use_after_acquire, in_use_after_release
            "#,
            )
            .eval()
            .unwrap();
        assert_eq!(result.0, 2);
        assert_eq!(result.1, 1);
        assert_eq!(result.2, 0);
    }

    #[test]
    fn test_message_type_constants() {
        let lua = setup_lua();

        let result: String = lua
            .load(
                r#"
                local msg_type = indras.iot.MessageType.sync_request()
                return tostring(msg_type)
            "#,
            )
            .eval()
            .unwrap();
        assert_eq!(result, "sync_request");
    }
}
