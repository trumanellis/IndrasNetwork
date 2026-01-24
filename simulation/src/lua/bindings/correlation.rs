//! Lua bindings for CorrelationContext
//!
//! Provides correlation context for distributed tracing from Lua scripts.

use mlua::{Lua, MetaMethod, Result, Table, UserData, UserDataMethods};

use indras_logging::CorrelationContext;

/// Lua wrapper for CorrelationContext
#[derive(Debug, Clone)]
pub struct LuaCorrelationContext(pub CorrelationContext);

impl From<CorrelationContext> for LuaCorrelationContext {
    fn from(ctx: CorrelationContext) -> Self {
        Self(ctx)
    }
}

impl UserData for LuaCorrelationContext {
    fn add_fields<F: mlua::UserDataFields<Self>>(fields: &mut F) {
        // trace_id - root trace identifier (string)
        fields.add_field_method_get("trace_id", |_, this| Ok(this.0.trace_id_str()));

        // span_id - current span identifier (string)
        fields.add_field_method_get("span_id", |_, this| Ok(this.0.span_id_str()));

        // parent_span_id - parent span identifier (string or nil)
        fields.add_field_method_get("parent_span_id", |_, this| Ok(this.0.parent_span_id_str()));

        // packet_id - associated packet ID (string or nil)
        fields.add_field_method_get("packet_id", |_, this| Ok(this.0.packet_id.clone()));

        // hop_count - current hop count
        fields.add_field_method_get("hop_count", |_, this| Ok(this.0.hop_count));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // child() -> CorrelationContext
        // Create a child context for relay/propagation
        methods.add_method("child", |_, this, ()| {
            Ok(LuaCorrelationContext(this.0.child()))
        });

        // with_packet_id(id) -> CorrelationContext
        // Attach a packet ID to this context
        methods.add_method("with_packet_id", |_, this, id: String| {
            Ok(LuaCorrelationContext(this.0.clone().with_packet_id(id)))
        });

        // with_tag(key, value) -> CorrelationContext
        // Create a copy with additional metadata (stored in packet_id for now)
        methods.add_method("with_tag", |_, this, (key, value): (String, String)| {
            let mut ctx = this.0.clone();
            let tag = format!("{}={}", key, value);
            if let Some(ref pid) = ctx.packet_id {
                ctx.packet_id = Some(format!("{}|{}", pid, tag));
            } else {
                ctx.packet_id = Some(tag);
            }
            Ok(LuaCorrelationContext(ctx))
        });

        // to_traceparent() -> string
        // Convert to W3C trace context format
        methods.add_method("to_traceparent", |_, this, ()| Ok(this.0.to_traceparent()));

        // to_table() -> table with all fields
        methods.add_method("to_table", |lua, this, ()| {
            let t = lua.create_table()?;
            t.set("trace_id", this.0.trace_id_str())?;
            t.set("span_id", this.0.span_id_str())?;
            if let Some(parent) = this.0.parent_span_id_str() {
                t.set("parent_span_id", parent)?;
            }
            if let Some(ref packet_id) = this.0.packet_id {
                t.set("packet_id", packet_id.clone())?;
            }
            t.set("hop_count", this.0.hop_count)?;
            Ok(t)
        });

        // String representation
        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            Ok(format!(
                "CorrelationContext(trace={}, hop={})",
                &this.0.trace_id_str()[..8],
                this.0.hop_count
            ))
        });
    }
}

/// Register correlation context constructors with the indras table
pub fn register(lua: &Lua, indras: &Table) -> Result<()> {
    let correlation = lua.create_table()?;

    // correlation.new_root() -> CorrelationContext
    // Create a new root context (at message origin)
    correlation.set(
        "new_root",
        lua.create_function(|_, ()| Ok(LuaCorrelationContext(CorrelationContext::new_root())))?,
    )?;

    // correlation.from_traceparent(traceparent) -> CorrelationContext or nil
    // Parse from W3C trace context format
    correlation.set(
        "from_traceparent",
        lua.create_function(|_, traceparent: String| {
            Ok(CorrelationContext::from_traceparent(&traceparent).map(LuaCorrelationContext))
        })?,
    )?;

    indras.set("correlation", correlation)?;

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
    fn test_new_root() {
        let lua = setup_lua();

        let hop: u32 = lua
            .load(
                r#"
                local ctx = indras.correlation.new_root()
                return ctx.hop_count
            "#,
            )
            .eval()
            .unwrap();
        assert_eq!(hop, 0);
    }

    #[test]
    fn test_child() {
        let lua = setup_lua();

        let (same_trace, diff_span, hop): (bool, bool, u32) = lua
            .load(
                r#"
                local root = indras.correlation.new_root()
                local child = root:child()
                return root.trace_id == child.trace_id,
                       root.span_id ~= child.span_id,
                       child.hop_count
            "#,
            )
            .eval()
            .unwrap();
        assert!(same_trace);
        assert!(diff_span);
        assert_eq!(hop, 1);
    }

    #[test]
    fn test_with_packet_id() {
        let lua = setup_lua();

        let packet_id: String = lua
            .load(
                r#"
                local ctx = indras.correlation.new_root():with_packet_id("A#1")
                return ctx.packet_id
            "#,
            )
            .eval()
            .unwrap();
        assert_eq!(packet_id, "A#1");
    }

    #[test]
    fn test_to_traceparent() {
        let lua = setup_lua();

        let traceparent: String = lua
            .load(
                r#"
                local ctx = indras.correlation.new_root()
                return ctx:to_traceparent()
            "#,
            )
            .eval()
            .unwrap();
        assert!(traceparent.starts_with("00-"));
    }

    #[test]
    fn test_from_traceparent() {
        let lua = setup_lua();

        // First generate one, then parse it
        let result: bool = lua
            .load(
                r#"
                local ctx1 = indras.correlation.new_root()
                local tp = ctx1:to_traceparent()
                local ctx2 = indras.correlation.from_traceparent(tp)
                return ctx2 ~= nil and ctx1.trace_id == ctx2.trace_id
            "#,
            )
            .eval()
            .unwrap();
        assert!(result);
    }

    #[test]
    fn test_to_table() {
        let lua = setup_lua();

        let has_trace_id: bool = lua
            .load(
                r#"
                local ctx = indras.correlation.new_root()
                local t = ctx:to_table()
                return t.trace_id ~= nil
            "#,
            )
            .eval()
            .unwrap();
        assert!(has_trace_id);
    }
}
