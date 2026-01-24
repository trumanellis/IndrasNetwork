//! Lua bindings for JSONL logging
//!
//! Provides structured logging from Lua scripts that integrates with
//! the indras-logging JSONL system.

use mlua::{Lua, ObjectLike, Result, Table, Value};
use std::collections::HashMap;
use tracing::{debug, error, info, trace, warn};

/// Convert a Lua table to a HashMap for logging fields
fn table_to_fields(table: &Table) -> Result<HashMap<String, serde_json::Value>> {
    let mut fields = HashMap::new();

    for pair in table.pairs::<String, Value>() {
        let (key, value) = pair?;
        let json_value = lua_value_to_json(&value)?;
        fields.insert(key, json_value);
    }

    Ok(fields)
}

/// Convert a Lua value to a serde_json Value
fn lua_value_to_json(value: &Value) -> Result<serde_json::Value> {
    match value {
        Value::Nil => Ok(serde_json::Value::Null),
        Value::Boolean(b) => Ok(serde_json::Value::Bool(*b)),
        Value::Integer(i) => Ok(serde_json::Value::Number((*i).into())),
        Value::Number(n) => serde_json::Number::from_f64(*n)
            .map(serde_json::Value::Number)
            .ok_or_else(|| mlua::Error::external("Invalid float value")),
        Value::String(s) => Ok(serde_json::Value::String(s.to_str()?.to_string())),
        Value::Table(t) => {
            // Check if it's an array (sequential integer keys starting at 1)
            let is_array = t.clone().pairs::<i64, Value>().all(|r| r.is_ok());
            let first_key: mlua::Result<i64> = t.get(1);

            if is_array && first_key.is_ok() {
                let arr: Vec<serde_json::Value> = t
                    .sequence_values::<Value>()
                    .map(|v| v.and_then(|v| lua_value_to_json(&v)))
                    .collect::<Result<_>>()?;
                Ok(serde_json::Value::Array(arr))
            } else {
                let obj: serde_json::Map<String, serde_json::Value> = t
                    .pairs::<String, Value>()
                    .map(|pair| {
                        let (k, v) = pair?;
                        let json_v = lua_value_to_json(&v)?;
                        Ok((k, json_v))
                    })
                    .collect::<Result<_>>()?;
                Ok(serde_json::Value::Object(obj))
            }
        }
        Value::UserData(ud) => {
            // Try to convert to string
            if let Ok(s) = ud.to_string() {
                Ok(serde_json::Value::String(s))
            } else {
                Ok(serde_json::Value::String("<userdata>".to_string()))
            }
        }
        _ => Ok(serde_json::Value::String("<unsupported>".to_string())),
    }
}

/// Log a message with optional fields at a given level
fn log_with_fields(level: &str, msg: &str, fields: Option<HashMap<String, serde_json::Value>>) {
    let fields_str = fields
        .map(|f| serde_json::to_string(&f).unwrap_or_default())
        .unwrap_or_default();

    match level {
        "trace" => {
            if fields_str.is_empty() {
                trace!(source = "lua", "{}", msg);
            } else {
                trace!(source = "lua", fields = %fields_str, "{}", msg);
            }
        }
        "debug" => {
            if fields_str.is_empty() {
                debug!(source = "lua", "{}", msg);
            } else {
                debug!(source = "lua", fields = %fields_str, "{}", msg);
            }
        }
        "info" => {
            if fields_str.is_empty() {
                info!(source = "lua", "{}", msg);
            } else {
                info!(source = "lua", fields = %fields_str, "{}", msg);
            }
        }
        "warn" => {
            if fields_str.is_empty() {
                warn!(source = "lua", "{}", msg);
            } else {
                warn!(source = "lua", fields = %fields_str, "{}", msg);
            }
        }
        "error" => {
            if fields_str.is_empty() {
                error!(source = "lua", "{}", msg);
            } else {
                error!(source = "lua", fields = %fields_str, "{}", msg);
            }
        }
        _ => {
            info!(source = "lua", "{}", msg);
        }
    }
}

/// Register logging functions with the indras table
pub fn register(lua: &Lua, indras: &Table) -> Result<()> {
    let log = lua.create_table()?;

    // log.trace(msg, fields?)
    log.set(
        "trace",
        lua.create_function(|_, (msg, fields): (String, Option<Table>)| {
            let fields_map = fields.map(|t| table_to_fields(&t)).transpose()?;
            log_with_fields("trace", &msg, fields_map);
            Ok(())
        })?,
    )?;

    // log.debug(msg, fields?)
    log.set(
        "debug",
        lua.create_function(|_, (msg, fields): (String, Option<Table>)| {
            let fields_map = fields.map(|t| table_to_fields(&t)).transpose()?;
            log_with_fields("debug", &msg, fields_map);
            Ok(())
        })?,
    )?;

    // log.info(msg, fields?)
    log.set(
        "info",
        lua.create_function(|_, (msg, fields): (String, Option<Table>)| {
            let fields_map = fields.map(|t| table_to_fields(&t)).transpose()?;
            log_with_fields("info", &msg, fields_map);
            Ok(())
        })?,
    )?;

    // log.warn(msg, fields?)
    log.set(
        "warn",
        lua.create_function(|_, (msg, fields): (String, Option<Table>)| {
            let fields_map = fields.map(|t| table_to_fields(&t)).transpose()?;
            log_with_fields("warn", &msg, fields_map);
            Ok(())
        })?,
    )?;

    // log.error(msg, fields?)
    log.set(
        "error",
        lua.create_function(|_, (msg, fields): (String, Option<Table>)| {
            let fields_map = fields.map(|t| table_to_fields(&t)).transpose()?;
            log_with_fields("error", &msg, fields_map);
            Ok(())
        })?,
    )?;

    // log.print(msg) - simple print without structured fields
    log.set(
        "print",
        lua.create_function(|_, msg: String| {
            println!("{}", msg);
            Ok(())
        })?,
    )?;

    indras.set("log", log)?;

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
    fn test_log_simple() {
        let lua = setup_lua();
        // Just verify it doesn't panic
        lua.load(r#"indras.log.info("Hello from Lua")"#)
            .exec()
            .unwrap();
    }

    #[test]
    fn test_log_with_fields() {
        let lua = setup_lua();
        lua.load(
            r#"
            indras.log.info("Test message", {
                scenario = "test",
                count = 42,
                enabled = true
            })
        "#,
        )
        .exec()
        .unwrap();
    }

    #[test]
    fn test_log_levels() {
        let lua = setup_lua();
        lua.load(
            r#"
            indras.log.trace("trace message")
            indras.log.debug("debug message")
            indras.log.info("info message")
            indras.log.warn("warn message")
            indras.log.error("error message")
        "#,
        )
        .exec()
        .unwrap();
    }

    #[test]
    fn test_lua_value_to_json() {
        let _lua = Lua::new();

        // Test nil
        let v = lua_value_to_json(&Value::Nil).unwrap();
        assert_eq!(v, serde_json::Value::Null);

        // Test boolean
        let v = lua_value_to_json(&Value::Boolean(true)).unwrap();
        assert_eq!(v, serde_json::Value::Bool(true));

        // Test integer
        let v = lua_value_to_json(&Value::Integer(42)).unwrap();
        assert_eq!(v, serde_json::json!(42));

        // Test number
        let v = lua_value_to_json(&Value::Number(1.23456)).unwrap();
        assert!((v.as_f64().unwrap() - 1.23456).abs() < f64::EPSILON);
    }
}
