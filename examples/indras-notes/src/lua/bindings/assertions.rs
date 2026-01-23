//! Assertion helpers for Lua test scripts
//!
//! Provides test assertion functions similar to Lua's built-in assert
//! but with better error messages for testing.

use mlua::{Lua, Result, Table, Value};

/// Register assertion helpers with the notes table
pub fn register(lua: &Lua, notes: &Table) -> Result<()> {
    let assert_table = lua.create_table()?;

    // assert.eq(a, b, msg?) - assert equality
    assert_table.set(
        "eq",
        lua.create_function(|_, (a, b, msg): (Value, Value, Option<String>)| {
            if !values_equal(&a, &b) {
                let message = msg
                    .unwrap_or_else(|| format!("Assertion failed: {:?} == {:?}", a, b));
                return Err(mlua::Error::external(message));
            }
            Ok(())
        })?,
    )?;

    // assert.ne(a, b, msg?) - assert inequality
    assert_table.set(
        "ne",
        lua.create_function(|_, (a, b, msg): (Value, Value, Option<String>)| {
            if values_equal(&a, &b) {
                let message = msg
                    .unwrap_or_else(|| format!("Assertion failed: {:?} ~= {:?}", a, b));
                return Err(mlua::Error::external(message));
            }
            Ok(())
        })?,
    )?;

    // assert.gt(a, b, msg?) - assert a > b
    assert_table.set(
        "gt",
        lua.create_function(|_, (a, b, msg): (f64, f64, Option<String>)| {
            if a <= b {
                let message =
                    msg.unwrap_or_else(|| format!("Assertion failed: {} > {}", a, b));
                return Err(mlua::Error::external(message));
            }
            Ok(())
        })?,
    )?;

    // assert.ge(a, b, msg?) - assert a >= b
    assert_table.set(
        "ge",
        lua.create_function(|_, (a, b, msg): (f64, f64, Option<String>)| {
            if a < b {
                let message =
                    msg.unwrap_or_else(|| format!("Assertion failed: {} >= {}", a, b));
                return Err(mlua::Error::external(message));
            }
            Ok(())
        })?,
    )?;

    // assert.lt(a, b, msg?) - assert a < b
    assert_table.set(
        "lt",
        lua.create_function(|_, (a, b, msg): (f64, f64, Option<String>)| {
            if a >= b {
                let message =
                    msg.unwrap_or_else(|| format!("Assertion failed: {} < {}", a, b));
                return Err(mlua::Error::external(message));
            }
            Ok(())
        })?,
    )?;

    // assert.le(a, b, msg?) - assert a <= b
    assert_table.set(
        "le",
        lua.create_function(|_, (a, b, msg): (f64, f64, Option<String>)| {
            if a > b {
                let message =
                    msg.unwrap_or_else(|| format!("Assertion failed: {} <= {}", a, b));
                return Err(mlua::Error::external(message));
            }
            Ok(())
        })?,
    )?;

    // assert.true_(cond, msg?) - assert condition is true
    assert_table.set(
        "true_",
        lua.create_function(|_, (cond, msg): (bool, Option<String>)| {
            if !cond {
                let message =
                    msg.unwrap_or_else(|| "Assertion failed: expected true".to_string());
                return Err(mlua::Error::external(message));
            }
            Ok(())
        })?,
    )?;

    // assert.false_(cond, msg?) - assert condition is false
    assert_table.set(
        "false_",
        lua.create_function(|_, (cond, msg): (bool, Option<String>)| {
            if cond {
                let message =
                    msg.unwrap_or_else(|| "Assertion failed: expected false".to_string());
                return Err(mlua::Error::external(message));
            }
            Ok(())
        })?,
    )?;

    // assert.nil_(val, msg?) - assert value is nil
    assert_table.set(
        "nil_",
        lua.create_function(|_, (val, msg): (Value, Option<String>)| {
            if !matches!(val, Value::Nil) {
                let message = msg
                    .unwrap_or_else(|| format!("Assertion failed: expected nil, got {:?}", val));
                return Err(mlua::Error::external(message));
            }
            Ok(())
        })?,
    )?;

    // assert.not_nil(val, msg?) - assert value is not nil
    assert_table.set(
        "not_nil",
        lua.create_function(|_, (val, msg): (Value, Option<String>)| {
            if matches!(val, Value::Nil) {
                let message =
                    msg.unwrap_or_else(|| "Assertion failed: expected non-nil".to_string());
                return Err(mlua::Error::external(message));
            }
            Ok(())
        })?,
    )?;

    // assert.approx(a, b, epsilon?, msg?) - assert approximate equality for floats
    assert_table.set(
        "approx",
        lua.create_function(
            |_, (a, b, epsilon, msg): (f64, f64, Option<f64>, Option<String>)| {
                let eps = epsilon.unwrap_or(1e-9);
                if (a - b).abs() > eps {
                    let message = msg
                        .unwrap_or_else(|| format!("Assertion failed: {} ~= {} (epsilon={})", a, b, eps));
                    return Err(mlua::Error::external(message));
                }
                Ok(())
            },
        )?,
    )?;

    // assert.contains(table, value, msg?) - assert table contains value
    assert_table.set(
        "contains",
        lua.create_function(|_, (table, value, msg): (Table, Value, Option<String>)| {
            let mut found = false;
            for pair in table.pairs::<Value, Value>() {
                let (_, v) = pair?;
                if values_equal(&v, &value) {
                    found = true;
                    break;
                }
            }
            if !found {
                let message = msg
                    .unwrap_or_else(|| format!("Assertion failed: table does not contain {:?}", value));
                return Err(mlua::Error::external(message));
            }
            Ok(())
        })?,
    )?;

    // assert.len(table, expected_len, msg?) - assert table length
    assert_table.set(
        "len",
        lua.create_function(|_, (table, expected_len, msg): (Table, i64, Option<String>)| {
            let actual_len = table.len()?;
            if actual_len != expected_len {
                let message = msg.unwrap_or_else(|| {
                    format!(
                        "Assertion failed: expected length {}, got {}",
                        expected_len, actual_len
                    )
                });
                return Err(mlua::Error::external(message));
            }
            Ok(())
        })?,
    )?;

    // assert.fail(msg) - always fail with message
    assert_table.set(
        "fail",
        lua.create_function(|_, msg: String| Err::<(), _>(mlua::Error::external(msg)))?,
    )?;

    // assert.str_contains(haystack, needle, msg?) - assert string contains substring
    assert_table.set(
        "str_contains",
        lua.create_function(
            |_, (haystack, needle, msg): (String, String, Option<String>)| {
                if !haystack.contains(&needle) {
                    let message = msg.unwrap_or_else(|| {
                        format!(
                            "Assertion failed: '{}' does not contain '{}'",
                            haystack, needle
                        )
                    });
                    return Err(mlua::Error::external(message));
                }
                Ok(())
            },
        )?,
    )?;

    // assert.str_starts_with(str, prefix, msg?) - assert string starts with prefix
    assert_table.set(
        "str_starts_with",
        lua.create_function(
            |_, (s, prefix, msg): (String, String, Option<String>)| {
                if !s.starts_with(&prefix) {
                    let message = msg.unwrap_or_else(|| {
                        format!(
                            "Assertion failed: '{}' does not start with '{}'",
                            s, prefix
                        )
                    });
                    return Err(mlua::Error::external(message));
                }
                Ok(())
            },
        )?,
    )?;

    notes.set("assert", assert_table)?;

    Ok(())
}

/// Compare two Lua values for equality
fn values_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Nil, Value::Nil) => true,
        (Value::Boolean(a), Value::Boolean(b)) => a == b,
        (Value::Integer(a), Value::Integer(b)) => a == b,
        (Value::Integer(a), Value::Number(b)) => (*a as f64 - b).abs() < f64::EPSILON,
        (Value::Number(a), Value::Integer(b)) => (a - *b as f64).abs() < f64::EPSILON,
        (Value::Number(a), Value::Number(b)) => (a - b).abs() < f64::EPSILON,
        (Value::String(a), Value::String(b)) => a.as_bytes() == b.as_bytes(),
        _ => false, // Complex types require more sophisticated comparison
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_lua() -> Lua {
        let lua = Lua::new();
        let notes = lua.create_table().unwrap();
        register(&lua, &notes).unwrap();
        lua.globals().set("notes", notes).unwrap();
        lua
    }

    #[test]
    fn test_assert_eq_pass() {
        let lua = setup_lua();
        lua.load("notes.assert.eq(1, 1)").exec().unwrap();
        lua.load("notes.assert.eq('hello', 'hello')").exec().unwrap();
    }

    #[test]
    fn test_assert_eq_fail() {
        let lua = setup_lua();
        let result = lua.load("notes.assert.eq(1, 2)").exec();
        assert!(result.is_err());
    }

    #[test]
    fn test_assert_ne_pass() {
        let lua = setup_lua();
        lua.load("notes.assert.ne(1, 2)").exec().unwrap();
    }

    #[test]
    fn test_assert_gt_pass() {
        let lua = setup_lua();
        lua.load("notes.assert.gt(2, 1)").exec().unwrap();
    }

    #[test]
    fn test_assert_gt_fail() {
        let lua = setup_lua();
        let result = lua.load("notes.assert.gt(1, 2)").exec();
        assert!(result.is_err());
    }

    #[test]
    fn test_assert_true() {
        let lua = setup_lua();
        lua.load("notes.assert.true_(true)").exec().unwrap();
        let result = lua.load("notes.assert.true_(false)").exec();
        assert!(result.is_err());
    }

    #[test]
    fn test_assert_nil() {
        let lua = setup_lua();
        lua.load("notes.assert.nil_(nil)").exec().unwrap();
        let result = lua.load("notes.assert.nil_(1)").exec();
        assert!(result.is_err());
    }

    #[test]
    fn test_assert_not_nil() {
        let lua = setup_lua();
        lua.load("notes.assert.not_nil(1)").exec().unwrap();
        lua.load("notes.assert.not_nil('hello')").exec().unwrap();
        let result = lua.load("notes.assert.not_nil(nil)").exec();
        assert!(result.is_err());
    }

    #[test]
    fn test_assert_approx() {
        let lua = setup_lua();
        lua.load("notes.assert.approx(1.0, 1.0000001, 0.001)")
            .exec()
            .unwrap();
        let result = lua.load("notes.assert.approx(1.0, 2.0, 0.001)").exec();
        assert!(result.is_err());
    }

    #[test]
    fn test_assert_contains() {
        let lua = setup_lua();
        lua.load("notes.assert.contains({1, 2, 3}, 2)")
            .exec()
            .unwrap();
        let result = lua.load("notes.assert.contains({1, 2, 3}, 4)").exec();
        assert!(result.is_err());
    }

    #[test]
    fn test_assert_len() {
        let lua = setup_lua();
        lua.load("notes.assert.len({1, 2, 3}, 3)").exec().unwrap();
        let result = lua.load("notes.assert.len({1, 2, 3}, 2)").exec();
        assert!(result.is_err());
    }

    #[test]
    fn test_assert_fail() {
        let lua = setup_lua();
        let result = lua.load("notes.assert.fail('intentional failure')").exec();
        assert!(result.is_err());
    }

    #[test]
    fn test_str_contains() {
        let lua = setup_lua();
        lua.load("notes.assert.str_contains('hello world', 'world')")
            .exec()
            .unwrap();
        let result = lua
            .load("notes.assert.str_contains('hello', 'world')")
            .exec();
        assert!(result.is_err());
    }

    #[test]
    fn test_custom_message() {
        let lua = setup_lua();
        let result = lua
            .load("notes.assert.eq(1, 2, 'custom error message')")
            .exec();
        let err = result.unwrap_err();
        assert!(err.to_string().contains("custom error message"));
    }
}
