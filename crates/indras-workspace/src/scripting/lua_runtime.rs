//! Lua test runtime — mlua setup, bindings, and sandbox.
//!
//! Runs on a dedicated OS thread. Communicates with Dioxus via channels.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use mlua::{Lua, Result, Value};
use tokio::sync::{broadcast, mpsc, oneshot};

use super::action::Action;
use super::event::AppEvent;
use super::query::{Query, QueryResult};

/// Lua test runtime with all indras.* bindings registered.
pub struct LuaTestRuntime {
    lua: Lua,
}

impl LuaTestRuntime {
    /// Create a new runtime with all bindings wired to the given channels.
    pub fn new(
        action_tx: mpsc::Sender<Action>,
        event_rx: broadcast::Receiver<AppEvent>,
        query_tx: mpsc::Sender<(Query, oneshot::Sender<QueryResult>)>,
        identity_name: Option<String>,
    ) -> Self {
        let lua = Lua::new();

        // Sandbox: remove dangerous globals
        sandbox_lua(&lua);

        let indras = lua.create_table().unwrap();

        // --- Identity ---
        let name_clone = identity_name.clone();
        indras
            .set(
                "my_name",
                lua.create_function(move |_, ()| {
                    Ok(name_clone.clone().unwrap_or_else(|| "Unknown".to_string()))
                })
                .unwrap(),
            )
            .unwrap();

        let name_clone2 = identity_name;
        indras
            .set(
                "identity",
                lua.create_function(move |lua, ()| {
                    let t = lua.create_table()?;
                    t.set(
                        "name",
                        name_clone2.clone().unwrap_or_else(|| "Unknown".to_string()),
                    )?;
                    Ok(t)
                })
                .unwrap(),
            )
            .unwrap();

        // --- Actions ---

        // indras.action(name, arg?)
        let tx = action_tx.clone();
        indras
            .set(
                "action",
                lua.create_function(move |_, (name, arg): (String, Option<String>)| {
                    let action =
                        Action::parse(&name, arg).map_err(|e| mlua::Error::external(e))?;
                    tx.blocking_send(action)
                        .map_err(|e| mlua::Error::external(e))?;
                    Ok(())
                })
                .unwrap(),
            )
            .unwrap();

        // Convenience action wrappers
        register_action_wrapper(&lua, &indras, &action_tx, "click_sidebar", true);
        register_action_wrapper(&lua, &indras, &action_tx, "click_tab", true);
        register_action_wrapper(&lua, &indras, &action_tx, "click_peer", true);
        register_action_wrapper(&lua, &indras, &action_tx, "open_contacts", false);
        register_action_wrapper(&lua, &indras, &action_tx, "paste_code", false);
        register_action_wrapper(&lua, &indras, &action_tx, "click_connect", false);
        register_action_wrapper(&lua, &indras, &action_tx, "close_overlay", false);
        register_action_wrapper(&lua, &indras, &action_tx, "type_message", true);
        register_action_wrapper(&lua, &indras, &action_tx, "send_message", false);
        register_action_wrapper(&lua, &indras, &action_tx, "set_name", false);
        register_action_wrapper(&lua, &indras, &action_tx, "create_identity", false);
        register_action_wrapper(&lua, &indras, &action_tx, "open_slash_menu", false);

        // indras.paste_code(uri) -> paste_connect_code
        let tx = action_tx.clone();
        indras
            .set(
                "paste_code",
                lua.create_function(move |_, code: String| {
                    tx.blocking_send(Action::PasteConnectCode(code))
                        .map_err(|e| mlua::Error::external(e))?;
                    Ok(())
                })
                .unwrap(),
            )
            .unwrap();

        // indras.set_name(name) -> SetDisplayName
        let tx = action_tx.clone();
        indras
            .set(
                "set_name",
                lua.create_function(move |_, name: String| {
                    tx.blocking_send(Action::SetDisplayName(name))
                        .map_err(|e| mlua::Error::external(e))?;
                    Ok(())
                })
                .unwrap(),
            )
            .unwrap();

        // indras.create_identity() -> ClickCreateIdentity
        let tx = action_tx.clone();
        indras
            .set(
                "create_identity",
                lua.create_function(move |_, ()| {
                    tx.blocking_send(Action::ClickCreateIdentity)
                        .map_err(|e| mlua::Error::external(e))?;
                    Ok(())
                })
                .unwrap(),
            )
            .unwrap();

        // indras.wait(seconds)
        let tx = action_tx.clone();
        indras
            .set(
                "wait",
                lua.create_function(move |_, secs: f64| {
                    tx.blocking_send(Action::Wait(secs))
                        .map_err(|e| mlua::Error::external(e))?;
                    // Also sleep on the Lua thread side
                    std::thread::sleep(Duration::from_secs_f64(secs));
                    Ok(())
                })
                .unwrap(),
            )
            .unwrap();

        // --- Events ---

        // indras.wait_for(event_name, filter?, timeout_secs?)
        let event_rx = Mutex::new(event_rx);
        indras
            .set(
                "wait_for",
                lua.create_function(
                    move |_, (event_name, filter, timeout): (String, Option<String>, Option<f64>)| {
                        let timeout = Duration::from_secs_f64(timeout.unwrap_or(30.0));
                        let deadline = Instant::now() + timeout;

                        loop {
                            match event_rx.lock().unwrap().try_recv() {
                                Ok(evt) => {
                                    if evt.matches(&event_name, filter.as_deref()) {
                                        return Ok(true);
                                    }
                                }
                                Err(broadcast::error::TryRecvError::Empty) => {
                                    if Instant::now() > deadline {
                                        return Err(mlua::Error::external(format!(
                                            "Timeout waiting for event '{}' after {:.1}s",
                                            event_name,
                                            timeout.as_secs_f64()
                                        )));
                                    }
                                    std::thread::sleep(Duration::from_millis(50));
                                }
                                Err(broadcast::error::TryRecvError::Lagged(_)) => {
                                    continue;
                                }
                                Err(broadcast::error::TryRecvError::Closed) => {
                                    return Err(mlua::Error::external(
                                        "Event channel closed",
                                    ));
                                }
                            }
                        }
                    },
                )
                .unwrap(),
            )
            .unwrap();

        // --- Queries ---

        // indras.query(name)
        let qtx = query_tx;
        indras
            .set(
                "query",
                lua.create_function(move |lua, name: String| {
                    let query =
                        Query::parse(&name).map_err(|e| mlua::Error::external(e))?;
                    let (tx, rx) = oneshot::channel();
                    qtx.blocking_send((query, tx))
                        .map_err(|e| mlua::Error::external(e))?;
                    let result = rx
                        .blocking_recv()
                        .map_err(|e| mlua::Error::external(e))?;
                    query_result_to_lua(lua, result)
                })
                .unwrap(),
            )
            .unwrap();

        // --- Assertions (reuse simulation pattern) ---
        register_assertions(&lua, &indras);

        // --- Logging (reuse simulation pattern) ---
        register_logging(&lua, &indras);

        lua.globals().set("indras", indras).unwrap();

        Self { lua }
    }

    /// Execute a Lua script from a file path.
    pub fn exec_file(&self, path: &str) -> Result<()> {
        let script = std::fs::read_to_string(path).map_err(|e| {
            mlua::Error::external(format!("Failed to read {}: {}", path, e))
        })?;
        self.lua.load(&script).set_name(path).exec()
    }

    /// Execute a Lua script from a string.
    pub fn exec(&self, script: &str) -> Result<()> {
        self.lua.load(script).exec()
    }
}

/// Remove dangerous Lua standard libraries for sandboxing.
fn sandbox_lua(lua: &Lua) {
    let globals = lua.globals();

    // Remove dangerous standard libraries
    globals.set("io", mlua::Value::Nil).ok();
    globals.set("loadfile", mlua::Value::Nil).ok();
    globals.set("dofile", mlua::Value::Nil).ok();

    // Replace os with safe subset
    let safe_os = lua.create_table().unwrap();
    safe_os
        .set(
            "getenv",
            lua.create_function(|_, key: String| Ok(std::env::var(key).ok()))
                .unwrap(),
        )
        .unwrap();
    safe_os
        .set(
            "clock",
            lua.create_function(|_, ()| {
                Ok(std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs_f64())
            })
            .unwrap(),
        )
        .unwrap();
    globals.set("os", safe_os).ok();
}

/// Register a convenience action wrapper: indras.<name>(arg?)
fn register_action_wrapper(
    lua: &Lua,
    indras: &mlua::Table,
    tx: &mpsc::Sender<Action>,
    lua_name: &str,
    has_arg: bool,
) {
    let tx = tx.clone();
    let action_name = lua_name.to_string();

    if has_arg {
        indras
            .set(
                lua_name,
                lua.create_function(move |_, arg: String| {
                    let action = Action::parse(&action_name, Some(arg))
                        .map_err(|e| mlua::Error::external(e))?;
                    tx.blocking_send(action)
                        .map_err(|e| mlua::Error::external(e))?;
                    Ok(())
                })
                .unwrap(),
            )
            .unwrap();
    } else {
        indras
            .set(
                lua_name,
                lua.create_function(move |_, ()| {
                    let action = Action::parse(&action_name, None)
                        .map_err(|e| mlua::Error::external(e))?;
                    tx.blocking_send(action)
                        .map_err(|e| mlua::Error::external(e))?;
                    Ok(())
                })
                .unwrap(),
            )
            .unwrap();
    }
}

/// Convert a QueryResult into a Lua value.
fn query_result_to_lua(lua: &Lua, result: QueryResult) -> Result<Value> {
    match result {
        QueryResult::String(s) => Ok(Value::String(lua.create_string(&s)?)),
        QueryResult::Number(n) => Ok(Value::Number(n)),
        QueryResult::StringList(list) => {
            let t = lua.create_table()?;
            for (i, s) in list.iter().enumerate() {
                t.set(i + 1, s.as_str())?;
            }
            Ok(Value::Table(t))
        }
        QueryResult::Json(val) => json_to_lua(lua, &val),
        QueryResult::Error(e) => Err(mlua::Error::external(e)),
    }
}

/// Convert a serde_json::Value to a Lua value.
fn json_to_lua(lua: &Lua, val: &serde_json::Value) -> Result<Value> {
    match val {
        serde_json::Value::Null => Ok(Value::Nil),
        serde_json::Value::Bool(b) => Ok(Value::Boolean(*b)),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(Value::Integer(i))
            } else {
                Ok(Value::Number(n.as_f64().unwrap_or(0.0)))
            }
        }
        serde_json::Value::String(s) => Ok(Value::String(lua.create_string(s)?)),
        serde_json::Value::Array(arr) => {
            let t = lua.create_table()?;
            for (i, v) in arr.iter().enumerate() {
                t.set(i + 1, json_to_lua(lua, v)?)?;
            }
            Ok(Value::Table(t))
        }
        serde_json::Value::Object(obj) => {
            let t = lua.create_table()?;
            for (k, v) in obj {
                t.set(k.as_str(), json_to_lua(lua, v)?)?;
            }
            Ok(Value::Table(t))
        }
    }
}

/// Register assertion helpers (mirrors simulation/src/lua/assertions.rs pattern).
fn register_assertions(lua: &Lua, indras: &mlua::Table) {
    let assert_table = lua.create_table().unwrap();

    // assert.eq(a, b, msg?)
    assert_table
        .set(
            "eq",
            lua.create_function(|_, (a, b, msg): (Value, Value, Option<String>)| {
                if !values_equal(&a, &b) {
                    let message =
                        msg.unwrap_or_else(|| format!("Assertion failed: {:?} == {:?}", a, b));
                    return Err(mlua::Error::external(message));
                }
                Ok(())
            })
            .unwrap(),
        )
        .unwrap();

    // assert.ne(a, b, msg?)
    assert_table
        .set(
            "ne",
            lua.create_function(|_, (a, b, msg): (Value, Value, Option<String>)| {
                if values_equal(&a, &b) {
                    let message =
                        msg.unwrap_or_else(|| format!("Assertion failed: {:?} ~= {:?}", a, b));
                    return Err(mlua::Error::external(message));
                }
                Ok(())
            })
            .unwrap(),
        )
        .unwrap();

    // assert.gt(a, b, msg?)
    assert_table
        .set(
            "gt",
            lua.create_function(|_, (a, b, msg): (f64, f64, Option<String>)| {
                if a <= b {
                    let message = msg.unwrap_or_else(|| format!("Assertion failed: {} > {}", a, b));
                    return Err(mlua::Error::external(message));
                }
                Ok(())
            })
            .unwrap(),
        )
        .unwrap();

    // assert.ge(a, b, msg?)
    assert_table
        .set(
            "ge",
            lua.create_function(|_, (a, b, msg): (f64, f64, Option<String>)| {
                if a < b {
                    let message =
                        msg.unwrap_or_else(|| format!("Assertion failed: {} >= {}", a, b));
                    return Err(mlua::Error::external(message));
                }
                Ok(())
            })
            .unwrap(),
        )
        .unwrap();

    // assert.lt(a, b, msg?)
    assert_table
        .set(
            "lt",
            lua.create_function(|_, (a, b, msg): (f64, f64, Option<String>)| {
                if a >= b {
                    let message = msg.unwrap_or_else(|| format!("Assertion failed: {} < {}", a, b));
                    return Err(mlua::Error::external(message));
                }
                Ok(())
            })
            .unwrap(),
        )
        .unwrap();

    // assert.truthy(val, msg?)
    assert_table
        .set(
            "truthy",
            lua.create_function(|_, (val, msg): (Value, Option<String>)| {
                let is_truthy = match &val {
                    Value::Nil => false,
                    Value::Boolean(b) => *b,
                    _ => true,
                };
                if !is_truthy {
                    let message =
                        msg.unwrap_or_else(|| "Assertion failed: expected truthy value".to_string());
                    return Err(mlua::Error::external(message));
                }
                Ok(())
            })
            .unwrap(),
        )
        .unwrap();

    // assert.contains(table, value, msg?)
    assert_table
        .set(
            "contains",
            lua.create_function(
                |_, (table, value, msg): (mlua::Table, Value, Option<String>)| {
                    let mut found = false;
                    for pair in table.pairs::<Value, Value>() {
                        let (_, v) = pair?;
                        if values_equal(&v, &value) {
                            found = true;
                            break;
                        }
                    }
                    if !found {
                        let message = msg.unwrap_or_else(|| {
                            format!("Assertion failed: table does not contain {:?}", value)
                        });
                        return Err(mlua::Error::external(message));
                    }
                    Ok(())
                },
            )
            .unwrap(),
        )
        .unwrap();

    indras.set("assert", assert_table).unwrap();
}

/// Compare two Lua values for equality.
fn values_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Nil, Value::Nil) => true,
        (Value::Boolean(a), Value::Boolean(b)) => a == b,
        (Value::Integer(a), Value::Integer(b)) => a == b,
        (Value::Integer(a), Value::Number(b)) => (*a as f64 - b).abs() < f64::EPSILON,
        (Value::Number(a), Value::Integer(b)) => (a - *b as f64).abs() < f64::EPSILON,
        (Value::Number(a), Value::Number(b)) => (a - b).abs() < f64::EPSILON,
        (Value::String(a), Value::String(b)) => a.as_bytes() == b.as_bytes(),
        _ => false,
    }
}

/// Register logging helpers (mirrors simulation/src/lua/bindings/logging.rs pattern).
fn register_logging(lua: &Lua, indras: &mlua::Table) {
    let log = lua.create_table().unwrap();

    log.set(
        "info",
        lua.create_function(|_, (msg, _fields): (String, Option<mlua::Table>)| {
            tracing::info!(source = "lua", "{}", msg);
            Ok(())
        })
        .unwrap(),
    )
    .unwrap();

    log.set(
        "debug",
        lua.create_function(|_, (msg, _fields): (String, Option<mlua::Table>)| {
            tracing::debug!(source = "lua", "{}", msg);
            Ok(())
        })
        .unwrap(),
    )
    .unwrap();

    log.set(
        "warn",
        lua.create_function(|_, (msg, _fields): (String, Option<mlua::Table>)| {
            tracing::warn!(source = "lua", "{}", msg);
            Ok(())
        })
        .unwrap(),
    )
    .unwrap();

    log.set(
        "error",
        lua.create_function(|_, (msg, _fields): (String, Option<mlua::Table>)| {
            tracing::error!(source = "lua", "{}", msg);
            Ok(())
        })
        .unwrap(),
    )
    .unwrap();

    indras.set("log", log).unwrap();
}
