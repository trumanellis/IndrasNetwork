//! Log assertion functions for Lua
//!
//! Provides assertions that verify log output patterns.

use mlua::{AnyUserData, Lua, Result, Table};

use crate::log_analysis::LogQuery;
use crate::log_capture::LogCapture;

/// Helper to get LogCapture from registry
fn get_capture(lua: &Lua) -> Result<LogCapture> {
    let ud: AnyUserData = lua.named_registry_value("log_capture")?;
    ud.borrow::<LogCapture>().map(|c| c.clone())
}

/// Register log assertion functions with the notes table
pub fn register(lua: &Lua, notes: &Table, capture: LogCapture) -> Result<()> {
    let log_assert = lua.create_table()?;

    // Store capture in Lua registry for access from functions
    lua.set_named_registry_value("log_capture", capture.clone())?;

    // log_assert.has_message(pattern) -> bool
    log_assert.set(
        "has_message",
        lua.create_function(|lua, pattern: String| {
            let capture = get_capture(lua)?;
            let lines = capture.get_logs();
            let query = LogQuery::from_lines(&lines);
            Ok(query.message_contains(&pattern).exists())
        })?,
    )?;

    // log_assert.count_level(level) -> int
    log_assert.set(
        "count_level",
        lua.create_function(|lua, level: String| {
            let capture = get_capture(lua)?;
            let lines = capture.get_logs();
            let query = LogQuery::from_lines(&lines);
            Ok(query.level(&level).count())
        })?,
    )?;

    // log_assert.no_errors() - assert no ERROR level logs
    log_assert.set(
        "no_errors",
        lua.create_function(|lua, ()| {
            let capture = get_capture(lua)?;
            let lines = capture.get_logs();
            let query = LogQuery::from_lines(&lines);
            let error_count = query.level("ERROR").count();
            if error_count > 0 {
                return Err(mlua::Error::external(format!(
                    "Expected no errors, but found {} ERROR log(s)",
                    error_count
                )));
            }
            Ok(())
        })?,
    )?;

    // log_assert.no_warnings() - assert no WARN level logs
    log_assert.set(
        "no_warnings",
        lua.create_function(|lua, ()| {
            let capture = get_capture(lua)?;
            let lines = capture.get_logs();
            let query = LogQuery::from_lines(&lines);
            let warn_count = query.level("WARN").count();
            if warn_count > 0 {
                return Err(mlua::Error::external(format!(
                    "Expected no warnings, but found {} WARN log(s)",
                    warn_count
                )));
            }
            Ok(())
        })?,
    )?;

    // log_assert.expect_sequence(patterns) - assert messages appear in order
    log_assert.set(
        "expect_sequence",
        lua.create_function(|lua, patterns: Table| {
            let capture = get_capture(lua)?;
            let lines = capture.get_logs();
            let query = LogQuery::from_lines(&lines);

            // Convert Lua table to Vec<String>
            let pattern_vec: Vec<String> = patterns
                .sequence_values::<String>()
                .collect::<mlua::Result<_>>()?;

            let pattern_refs: Vec<&str> = pattern_vec.iter().map(|s| s.as_str()).collect();

            if !query.messages_in_order(&pattern_refs) {
                return Err(mlua::Error::external(format!(
                    "Log messages did not appear in expected sequence: {:?}",
                    pattern_vec
                )));
            }
            Ok(())
        })?,
    )?;

    // log_assert.assert_message(pattern, msg?) - assert message exists or fail
    log_assert.set(
        "assert_message",
        lua.create_function(|lua, (pattern, msg): (String, Option<String>)| {
            let capture = get_capture(lua)?;
            let lines = capture.get_logs();
            let query = LogQuery::from_lines(&lines);

            if !query.message_contains(&pattern).exists() {
                let message = msg.unwrap_or_else(|| {
                    format!("Expected log message containing '{}' not found", pattern)
                });
                return Err(mlua::Error::external(message));
            }
            Ok(())
        })?,
    )?;

    // log_assert.clear() - clear captured logs
    log_assert.set(
        "clear",
        lua.create_function(|lua, ()| {
            let capture = get_capture(lua)?;
            capture.clear();
            Ok(())
        })?,
    )?;

    // log_assert.get_logs() -> table of log lines
    log_assert.set(
        "get_logs",
        lua.create_function(|lua, ()| {
            let capture = get_capture(lua)?;
            let lines = capture.get_logs();

            let table = lua.create_table()?;
            for (i, line) in lines.iter().enumerate() {
                table.set(i + 1, line.clone())?;
            }
            Ok(table)
        })?,
    )?;

    // log_assert.count() -> total number of log entries
    log_assert.set(
        "count",
        lua.create_function(|lua, ()| {
            let capture = get_capture(lua)?;
            Ok(capture.get_logs().len())
        })?,
    )?;

    notes.set("log_assert", log_assert)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tracing_subscriber::fmt::MakeWriter;

    fn setup_lua_with_capture() -> (Lua, LogCapture) {
        let lua = Lua::new();
        let notes = lua.create_table().unwrap();
        let capture = LogCapture::new();
        register(&lua, &notes, capture.clone()).unwrap();
        lua.globals().set("notes", notes).unwrap();
        (lua, capture)
    }

    #[test]
    fn test_has_message() {
        let (lua, capture) = setup_lua_with_capture();

        // Add some logs
        {
            let mut writer = capture.make_writer();
            writeln!(writer, r#"{{"level":"INFO","message":"Test message"}}"#).unwrap();
        }

        let found: bool = lua
            .load("return notes.log_assert.has_message('Test')")
            .eval()
            .unwrap();
        assert!(found);

        let not_found: bool = lua
            .load("return notes.log_assert.has_message('NotFound')")
            .eval()
            .unwrap();
        assert!(!not_found);
    }

    #[test]
    fn test_count_level() {
        let (lua, capture) = setup_lua_with_capture();

        {
            let mut writer = capture.make_writer();
            writeln!(writer, r#"{{"level":"INFO","message":"info1"}}"#).unwrap();
            writeln!(writer, r#"{{"level":"INFO","message":"info2"}}"#).unwrap();
            writeln!(writer, r#"{{"level":"ERROR","message":"error1"}}"#).unwrap();
        }

        let info_count: usize = lua
            .load("return notes.log_assert.count_level('INFO')")
            .eval()
            .unwrap();
        assert_eq!(info_count, 2);

        let error_count: usize = lua
            .load("return notes.log_assert.count_level('ERROR')")
            .eval()
            .unwrap();
        assert_eq!(error_count, 1);
    }

    #[test]
    fn test_no_errors_pass() {
        let (lua, capture) = setup_lua_with_capture();

        {
            let mut writer = capture.make_writer();
            writeln!(writer, r#"{{"level":"INFO","message":"all good"}}"#).unwrap();
        }

        lua.load("notes.log_assert.no_errors()").exec().unwrap();
    }

    #[test]
    fn test_no_errors_fail() {
        let (lua, capture) = setup_lua_with_capture();

        {
            let mut writer = capture.make_writer();
            writeln!(writer, r#"{{"level":"ERROR","message":"something bad"}}"#).unwrap();
        }

        let result = lua.load("notes.log_assert.no_errors()").exec();
        assert!(result.is_err());
    }

    #[test]
    fn test_expect_sequence() {
        let (lua, capture) = setup_lua_with_capture();

        {
            let mut writer = capture.make_writer();
            writeln!(writer, r#"{{"level":"INFO","message":"Step 1: Starting"}}"#).unwrap();
            writeln!(writer, r#"{{"level":"INFO","message":"Step 2: Processing"}}"#).unwrap();
            writeln!(writer, r#"{{"level":"INFO","message":"Step 3: Done"}}"#).unwrap();
        }

        // Should pass - messages in order
        lua.load("notes.log_assert.expect_sequence({'Step 1', 'Step 2', 'Step 3'})")
            .exec()
            .unwrap();

        // Should fail - messages out of order
        let result = lua
            .load("notes.log_assert.expect_sequence({'Step 3', 'Step 1'})")
            .exec();
        assert!(result.is_err());
    }

    #[test]
    fn test_clear() {
        let (lua, capture) = setup_lua_with_capture();

        {
            let mut writer = capture.make_writer();
            writeln!(writer, r#"{{"level":"INFO","message":"test"}}"#).unwrap();
        }

        assert!(!capture.get_logs().is_empty());

        lua.load("notes.log_assert.clear()").exec().unwrap();

        assert!(capture.get_logs().is_empty());
    }

    #[test]
    fn test_get_logs() {
        let (lua, capture) = setup_lua_with_capture();

        {
            let mut writer = capture.make_writer();
            writeln!(writer, r#"{{"level":"INFO","message":"line1"}}"#).unwrap();
            writeln!(writer, r#"{{"level":"INFO","message":"line2"}}"#).unwrap();
        }

        let count: usize = lua
            .load(
                r#"
            local logs = notes.log_assert.get_logs()
            return #logs
        "#,
            )
            .eval()
            .unwrap();
        assert_eq!(count, 2);
    }
}
