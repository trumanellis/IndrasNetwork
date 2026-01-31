//! Lua bindings for indras-network SyncEngine
//!
//! Provides Lua wrappers for the high-level SyncEngine types:
//! - IndrasNetwork - Main entry point for P2P applications
//! - Realm - Collaborative space for messaging and documents
//! - Document - CRDT-backed data structure (JSON values for Lua)
//! - Member - Identity wrapper
//! - InviteCode - Shareable realm invitations
//! - Preset - Configuration presets
//! - Content - Message content types

use mlua::{FromLua, Lua, LuaSerdeExt, MetaMethod, Result, Table, UserData, UserDataMethods, Value};
use std::sync::Arc;
use tokio::sync::RwLock;

use indras_logging::CorrelationContext;

use super::correlation::LuaCorrelationContext;

// Re-export base64 for encode/decode utilities
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};

// =============================================================================
// Mock types for simulation
// =============================================================================
//
// The indras-network crate requires tokio async runtime and real network
// infrastructure. For Lua scripting/testing, we provide mock types that
// simulate the SyncEngine behavior without actual network operations.

/// Configuration preset enum for SyncEngine
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LuaPreset {
    /// Balanced defaults for general use
    #[default]
    Default,
    /// Optimized for real-time chat applications
    Chat,
    /// Optimized for collaborative document editing
    Collaboration,
    /// Minimal footprint for IoT/embedded devices
    IoT,
    /// Maximum tolerance for disconnection
    OfflineFirst,
}

impl FromLua for LuaPreset {
    fn from_lua(value: Value, _lua: &Lua) -> Result<Self> {
        match value {
            Value::UserData(ud) => ud.borrow::<Self>().map(|v| *v),
            Value::String(s) => {
                let str_val: &str = &s.to_str()?;
                match str_val.to_lowercase().as_str() {
                    "default" => Ok(LuaPreset::Default),
                    "chat" => Ok(LuaPreset::Chat),
                    "collaboration" => Ok(LuaPreset::Collaboration),
                    "iot" => Ok(LuaPreset::IoT),
                    "offline_first" | "offlinefirst" => Ok(LuaPreset::OfflineFirst),
                    other => Err(mlua::Error::external(format!(
                        "Unknown preset: {}. Valid: default, chat, collaboration, iot, offline_first",
                        other
                    ))),
                }
            }
            _ => Err(mlua::Error::external(
                "Expected Preset userdata or string",
            )),
        }
    }
}

impl UserData for LuaPreset {
    fn add_fields<F: mlua::UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("name", |_, this| {
            Ok(match this {
                LuaPreset::Default => "default",
                LuaPreset::Chat => "chat",
                LuaPreset::Collaboration => "collaboration",
                LuaPreset::IoT => "iot",
                LuaPreset::OfflineFirst => "offline_first",
            })
        });

        fields.add_field_method_get("event_channel_capacity", |_, this| {
            Ok(match this {
                LuaPreset::Default => 1024,
                LuaPreset::Chat => 4096,
                LuaPreset::Collaboration => 2048,
                LuaPreset::IoT => 256,
                LuaPreset::OfflineFirst => 1024,
            })
        });

        fields.add_field_method_get("sync_interval_secs", |_, this| {
            Ok(match this {
                LuaPreset::Default => 5,
                LuaPreset::Chat => 1,
                LuaPreset::Collaboration => 2,
                LuaPreset::IoT => 30,
                LuaPreset::OfflineFirst => 5,
            })
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            Ok(match this {
                LuaPreset::Default => "Preset::Default",
                LuaPreset::Chat => "Preset::Chat",
                LuaPreset::Collaboration => "Preset::Collaboration",
                LuaPreset::IoT => "Preset::IoT",
                LuaPreset::OfflineFirst => "Preset::OfflineFirst",
            })
        });

        methods.add_meta_method(MetaMethod::Eq, |_, this, other: LuaPreset| {
            Ok(*this == other)
        });
    }
}

// =============================================================================
// Content enum binding
// =============================================================================

/// Message content types for Lua
#[derive(Debug, Clone)]
pub enum LuaContent {
    /// Plain text message
    Text(String),
    /// Binary data with MIME type
    Binary { mime_type: String, data: Vec<u8> },
    /// System message
    System(String),
    /// JSON data (for reactions, artifacts, etc.)
    Json(serde_json::Value),
}

impl FromLua for LuaContent {
    fn from_lua(value: Value, lua: &Lua) -> Result<Self> {
        match value {
            Value::UserData(ud) => ud.borrow::<Self>().map(|v| v.clone()),
            Value::String(s) => Ok(LuaContent::Text(s.to_str()?.to_string())),
            Value::Table(t) => {
                // Check for typed content table
                if let Ok(content_type) = t.get::<String>("type") {
                    match content_type.as_str() {
                        "text" => {
                            let text: String = t.get("text")?;
                            Ok(LuaContent::Text(text))
                        }
                        "binary" => {
                            let mime_type: String = t.get("mime_type")?;
                            let data: mlua::String = t.get("data")?;
                            Ok(LuaContent::Binary {
                                mime_type,
                                data: data.as_bytes().to_vec(),
                            })
                        }
                        "system" => {
                            let msg: String = t.get("message")?;
                            Ok(LuaContent::System(msg))
                        }
                        _ => {
                            // Convert table to JSON
                            let json = lua.from_value::<serde_json::Value>(Value::Table(t))?;
                            Ok(LuaContent::Json(json))
                        }
                    }
                } else {
                    // Plain table - convert to JSON
                    let json = lua.from_value::<serde_json::Value>(Value::Table(t))?;
                    Ok(LuaContent::Json(json))
                }
            }
            _ => Err(mlua::Error::external(
                "Expected Content userdata, string, or table",
            )),
        }
    }
}

impl UserData for LuaContent {
    fn add_fields<F: mlua::UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("content_type", |_, this| {
            Ok(match this {
                LuaContent::Text(_) => "text",
                LuaContent::Binary { .. } => "binary",
                LuaContent::System(_) => "system",
                LuaContent::Json(_) => "json",
            })
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // as_text() -> string or nil
        methods.add_method("as_text", |_, this, ()| {
            Ok(match this {
                LuaContent::Text(s) => Some(s.clone()),
                LuaContent::System(s) => Some(s.clone()),
                _ => None,
            })
        });

        // as_json() -> table or nil
        methods.add_method("as_json", |lua, this, ()| {
            match this {
                LuaContent::Json(v) => lua.to_value(v).map(Some),
                LuaContent::Text(s) => Ok(Some(Value::String(lua.create_string(s)?))),
                _ => Ok(None),
            }
        });

        // is_text() -> bool
        methods.add_method("is_text", |_, this, ()| {
            Ok(matches!(this, LuaContent::Text(_)))
        });

        // is_binary() -> bool
        methods.add_method("is_binary", |_, this, ()| {
            Ok(matches!(this, LuaContent::Binary { .. }))
        });

        // is_system() -> bool
        methods.add_method("is_system", |_, this, ()| {
            Ok(matches!(this, LuaContent::System(_)))
        });

        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            Ok(match this {
                LuaContent::Text(s) => format!("Content::Text(\"{}\")", truncate(s, 32)),
                LuaContent::Binary { mime_type, data } => {
                    format!("Content::Binary({}, {} bytes)", mime_type, data.len())
                }
                LuaContent::System(s) => format!("Content::System(\"{}\")", truncate(s, 32)),
                LuaContent::Json(v) => format!("Content::Json({})", v),
            })
        });
    }
}

// =============================================================================
// InviteCode binding
// =============================================================================

/// Lua wrapper for InviteCode
#[derive(Debug, Clone)]
pub struct LuaInviteCode {
    /// Base64-encoded invite data
    code: String,
    /// Realm ID (32-byte hex)
    realm_id: String,
}

impl LuaInviteCode {
    pub fn new(realm_id: &str) -> Self {
        // Generate a mock invite code
        let code = format!("indra:realm:{}", base64_encode(realm_id.as_bytes()));
        Self {
            code,
            realm_id: realm_id.to_string(),
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        let s = s.trim();
        if let Some(base64_part) = s.strip_prefix("indra:realm:") {
            let decoded = base64_decode(base64_part)
                .map_err(|e| mlua::Error::external(format!("Invalid base64: {}", e)))?;
            let realm_id = String::from_utf8_lossy(&decoded).to_string();
            Ok(Self {
                code: s.to_string(),
                realm_id,
            })
        } else if s.starts_with("indra:") {
            Err(mlua::Error::external(
                "Unknown invite type (expected 'realm')",
            ))
        } else {
            // Assume raw base64
            let decoded = base64_decode(s)
                .map_err(|e| mlua::Error::external(format!("Invalid base64: {}", e)))?;
            let realm_id = String::from_utf8_lossy(&decoded).to_string();
            Ok(Self {
                code: format!("indra:realm:{}", s),
                realm_id,
            })
        }
    }
}

impl FromLua for LuaInviteCode {
    fn from_lua(value: Value, _lua: &Lua) -> Result<Self> {
        match value {
            Value::UserData(ud) => ud.borrow::<Self>().map(|v| v.clone()),
            Value::String(s) => LuaInviteCode::parse(&s.to_str()?.to_string()),
            _ => Err(mlua::Error::external(
                "Expected InviteCode userdata or string",
            )),
        }
    }
}

impl UserData for LuaInviteCode {
    fn add_fields<F: mlua::UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("code", |_, this| Ok(this.code.clone()));
        fields.add_field_method_get("realm_id", |_, this| Ok(this.realm_id.clone()));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // to_uri() -> string
        methods.add_method("to_uri", |_, this, ()| Ok(this.code.clone()));

        // to_base64() -> string
        methods.add_method("to_base64", |_, this, ()| {
            Ok(this.code.strip_prefix("indra:realm:").unwrap_or(&this.code).to_string())
        });

        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            Ok(this.code.clone())
        });

        methods.add_meta_method(MetaMethod::Eq, |_, this, other: LuaInviteCode| {
            Ok(this.code == other.code)
        });
    }
}

// =============================================================================
// Member binding
// =============================================================================

/// Lua wrapper for Member identity
#[derive(Debug, Clone)]
pub struct LuaMember {
    /// 32-byte member ID (hex-encoded)
    id: String,
    /// Display name (optional)
    display_name: Option<String>,
}

impl LuaMember {
    pub fn new(id: &str) -> Self {
        Self {
            id: id.to_string(),
            display_name: None,
        }
    }

    pub fn with_name(id: &str, name: &str) -> Self {
        Self {
            id: id.to_string(),
            display_name: Some(name.to_string()),
        }
    }

    pub fn short_id(&self) -> String {
        if self.id.len() >= 8 {
            self.id[..8].to_string()
        } else {
            self.id.clone()
        }
    }
}

impl FromLua for LuaMember {
    fn from_lua(value: Value, _lua: &Lua) -> Result<Self> {
        match value {
            Value::UserData(ud) => ud.borrow::<Self>().map(|v| v.clone()),
            Value::String(s) => Ok(LuaMember::new(&s.to_str()?.to_string())),
            Value::Table(t) => {
                let id: String = t.get("id")?;
                let name: Option<String> = t.get("name").ok();
                Ok(LuaMember {
                    id,
                    display_name: name,
                })
            }
            _ => Err(mlua::Error::external(
                "Expected Member userdata, string, or table",
            )),
        }
    }
}

impl UserData for LuaMember {
    fn add_fields<F: mlua::UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("id", |_, this| Ok(this.id.clone()));
        fields.add_field_method_get("display_name", |_, this| Ok(this.display_name.clone()));
        fields.add_field_method_get("short_id", |_, this| Ok(this.short_id()));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // name() -> string (display name or short ID)
        methods.add_method("name", |_, this, ()| {
            Ok(this.display_name.clone().unwrap_or_else(|| this.short_id()))
        });

        // set_display_name(name)
        methods.add_method_mut("set_display_name", |_, this, name: Option<String>| {
            this.display_name = name;
            Ok(())
        });

        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            Ok(this.display_name.clone().unwrap_or_else(|| this.short_id()))
        });

        methods.add_meta_method(MetaMethod::Eq, |_, this, other: LuaMember| {
            Ok(this.id == other.id)
        });
    }
}

// =============================================================================
// Message binding
// =============================================================================

/// Lua wrapper for Message
#[derive(Debug, Clone)]
pub struct LuaMessage {
    /// Message ID
    pub id: String,
    /// Sender member
    pub sender: LuaMember,
    /// Message content
    pub content: LuaContent,
    /// Timestamp (Unix epoch millis)
    pub timestamp: i64,
    /// Reply-to message ID (optional)
    pub reply_to: Option<String>,
}

impl UserData for LuaMessage {
    fn add_fields<F: mlua::UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("id", |_, this| Ok(this.id.clone()));
        fields.add_field_method_get("sender", |_, this| Ok(this.sender.clone()));
        fields.add_field_method_get("content", |_, this| Ok(this.content.clone()));
        fields.add_field_method_get("timestamp", |_, this| Ok(this.timestamp));
        fields.add_field_method_get("reply_to", |_, this| Ok(this.reply_to.clone()));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // text() -> string or nil
        methods.add_method("text", |_, this, ()| {
            Ok(match &this.content {
                LuaContent::Text(s) => Some(s.clone()),
                _ => None,
            })
        });

        // is_reply() -> bool
        methods.add_method("is_reply", |_, this, ()| Ok(this.reply_to.is_some()));

        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            Ok(format!(
                "Message(id={}, sender={}, content={})",
                truncate(&this.id, 8),
                this.sender.short_id(),
                match &this.content {
                    LuaContent::Text(s) => format!("\"{}\"", truncate(s, 20)),
                    other => format!("{:?}", other),
                }
            ))
        });
    }
}

// =============================================================================
// Document binding (JSON-based for Lua)
// =============================================================================

/// Lua wrapper for Document<T> using JSON values
#[derive(Clone)]
pub struct LuaDocument {
    /// Document name
    name: String,
    /// Realm ID
    realm_id: String,
    /// Current document state (JSON)
    state: Arc<RwLock<serde_json::Value>>,
    /// Correlation context for tracing
    correlation: Option<CorrelationContext>,
}

impl LuaDocument {
    pub fn new(name: &str, realm_id: &str) -> Self {
        Self {
            name: name.to_string(),
            realm_id: realm_id.to_string(),
            state: Arc::new(RwLock::new(serde_json::Value::Object(Default::default()))),
            correlation: None,
        }
    }

    pub fn with_correlation(mut self, ctx: CorrelationContext) -> Self {
        self.correlation = Some(ctx);
        self
    }
}

impl UserData for LuaDocument {
    fn add_fields<F: mlua::UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("name", |_, this| Ok(this.name.clone()));
        fields.add_field_method_get("realm_id", |_, this| Ok(this.realm_id.clone()));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // read() -> table (blocking read of current state)
        methods.add_method("read", |lua, this, ()| {
            let state = this.state.blocking_read();
            lua.to_value(&*state)
        });

        // read_async() -> table (async read)
        methods.add_async_method("read_async", |lua, this, ()| async move {
            let state = this.state.read().await;
            if let Some(ref ctx) = this.correlation {
                tracing::debug!(
                    trace_id = %ctx.trace_id_str(),
                    document = %this.name,
                    "Document read"
                );
            }
            lua.to_value(&*state)
        });

        // set(key, value) -> updates a top-level key
        methods.add_async_method("set", |lua, this, (key, value): (String, Value)| async move {
            let json_value: serde_json::Value = lua.from_value(value)?;
            let mut state = this.state.write().await;
            if let serde_json::Value::Object(ref mut map) = *state {
                map.insert(key.clone(), json_value);
            }
            if let Some(ref ctx) = this.correlation {
                tracing::debug!(
                    trace_id = %ctx.trace_id_str(),
                    document = %this.name,
                    key = %key,
                    "Document updated"
                );
            }
            Ok(())
        });

        // get(key) -> value or nil
        methods.add_method("get", |lua, this, key: String| {
            let state = this.state.blocking_read();
            if let serde_json::Value::Object(ref map) = *state {
                if let Some(value) = map.get(&key) {
                    return lua.to_value(value);
                }
            }
            Ok(Value::Nil)
        });

        // update(table) -> merges table into document
        methods.add_async_method("update", |lua, this, updates: Table| async move {
            let json_updates: serde_json::Value = lua.from_value(Value::Table(updates))?;
            let mut state = this.state.write().await;
            if let (serde_json::Value::Object(current), serde_json::Value::Object(new)) =
                (&mut *state, json_updates)
            {
                for (k, v) in new {
                    current.insert(k, v);
                }
            }
            if let Some(ref ctx) = this.correlation {
                tracing::debug!(
                    trace_id = %ctx.trace_id_str(),
                    document = %this.name,
                    "Document batch update"
                );
            }
            Ok(())
        });

        // replace(table) -> replaces entire document state
        methods.add_async_method("replace", |lua, this, new_state: Table| async move {
            let json_state: serde_json::Value = lua.from_value(Value::Table(new_state))?;
            let mut state = this.state.write().await;
            *state = json_state;
            if let Some(ref ctx) = this.correlation {
                tracing::debug!(
                    trace_id = %ctx.trace_id_str(),
                    document = %this.name,
                    "Document replaced"
                );
            }
            Ok(())
        });

        // to_json() -> string
        methods.add_method("to_json", |_, this, ()| {
            let state = this.state.blocking_read();
            serde_json::to_string(&*state)
                .map_err(|e| mlua::Error::external(format!("JSON serialization failed: {}", e)))
        });

        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            Ok(format!(
                "Document(name=\"{}\", realm={})",
                this.name,
                truncate(&this.realm_id, 8)
            ))
        });
    }
}

// =============================================================================
// Realm binding
// =============================================================================

/// Lua wrapper for Realm
#[derive(Clone)]
pub struct LuaRealm {
    /// Realm ID
    id: String,
    /// Human-readable name
    name: Option<String>,
    /// Invite code
    invite: Option<LuaInviteCode>,
    /// Messages in this realm
    messages: Arc<RwLock<Vec<LuaMessage>>>,
    /// Documents in this realm
    documents: Arc<RwLock<std::collections::HashMap<String, LuaDocument>>>,
    /// Correlation context for tracing
    correlation: Option<CorrelationContext>,
}

impl LuaRealm {
    pub fn new(id: &str, name: Option<String>) -> Self {
        let invite = Some(LuaInviteCode::new(id));
        Self {
            id: id.to_string(),
            name,
            invite,
            messages: Arc::new(RwLock::new(Vec::new())),
            documents: Arc::new(RwLock::new(std::collections::HashMap::new())),
            correlation: None,
        }
    }

    pub fn with_correlation(mut self, ctx: CorrelationContext) -> Self {
        self.correlation = Some(ctx);
        self
    }
}

impl UserData for LuaRealm {
    fn add_fields<F: mlua::UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("id", |_, this| Ok(this.id.clone()));
        fields.add_field_method_get("name", |_, this| Ok(this.name.clone()));
        fields.add_field_method_get("invite_code", |_, this| Ok(this.invite.clone()));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // send(content) -> message_id
        methods.add_async_method("send", |_, this, content: LuaContent| async move {
            let msg_id = generate_id();
            let timestamp = current_timestamp();

            let message = LuaMessage {
                id: msg_id.clone(),
                sender: LuaMember::new("self"),
                content,
                timestamp,
                reply_to: None,
            };

            this.messages.write().await.push(message);

            if let Some(ref ctx) = this.correlation {
                tracing::debug!(
                    trace_id = %ctx.trace_id_str(),
                    realm = %this.id,
                    message_id = %msg_id,
                    "Message sent"
                );
            }

            Ok(msg_id)
        });

        // reply(reply_to_id, content) -> message_id
        methods.add_async_method(
            "reply",
            |_, this, (reply_to, content): (String, LuaContent)| async move {
                let msg_id = generate_id();
                let timestamp = current_timestamp();

                let message = LuaMessage {
                    id: msg_id.clone(),
                    sender: LuaMember::new("self"),
                    content,
                    timestamp,
                    reply_to: Some(reply_to),
                };

                this.messages.write().await.push(message);

                if let Some(ref ctx) = this.correlation {
                    tracing::debug!(
                        trace_id = %ctx.trace_id_str(),
                        realm = %this.id,
                        message_id = %msg_id,
                        "Reply sent"
                    );
                }

                Ok(msg_id)
            },
        );

        // messages_since(seq) -> [Message]
        methods.add_async_method("messages_since", |_, this, since: usize| async move {
            let messages = this.messages.read().await;
            let result: Vec<LuaMessage> = messages.iter().skip(since).cloned().collect();
            Ok(result)
        });

        // all_messages() -> [Message]
        methods.add_async_method("all_messages", |_, this, ()| async move {
            let messages = this.messages.read().await;
            Ok(messages.clone())
        });

        // message_count() -> number
        methods.add_async_method("message_count", |_, this, ()| async move {
            Ok(this.messages.read().await.len())
        });

        // document(name) -> Document
        methods.add_async_method("document", |_, this, name: String| async move {
            let mut docs = this.documents.write().await;
            if !docs.contains_key(&name) {
                let mut doc = LuaDocument::new(&name, &this.id);
                if let Some(ref ctx) = this.correlation {
                    doc = doc.with_correlation(ctx.child());
                }
                docs.insert(name.clone(), doc);
            }
            Ok(docs.get(&name).cloned())
        });

        // inject_message(sender_id, content) - for testing
        methods.add_async_method(
            "inject_message",
            |_, this, (sender_id, content): (String, LuaContent)| async move {
                let msg_id = generate_id();
                let timestamp = current_timestamp();

                let message = LuaMessage {
                    id: msg_id.clone(),
                    sender: LuaMember::new(&sender_id),
                    content,
                    timestamp,
                    reply_to: None,
                };

                this.messages.write().await.push(message);
                Ok(msg_id)
            },
        );

        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            Ok(format!(
                "Realm(id={}, name={:?})",
                truncate(&this.id, 8),
                this.name
            ))
        });
    }
}

// =============================================================================
// IndrasNetwork binding
// =============================================================================

/// Lua wrapper for IndrasNetwork (main SyncEngine entry point)
#[derive(Clone)]
pub struct LuaNetwork {
    /// Data directory
    data_dir: String,
    /// Display name
    display_name: Option<String>,
    /// Preset
    preset: LuaPreset,
    /// Our member identity
    identity: LuaMember,
    /// Active realms
    realms: Arc<RwLock<std::collections::HashMap<String, LuaRealm>>>,
    /// Is network running
    running: Arc<RwLock<bool>>,
    /// Correlation context for tracing
    correlation: Option<CorrelationContext>,
}

impl LuaNetwork {
    pub fn new(data_dir: &str) -> Self {
        let id = generate_id();
        Self {
            data_dir: data_dir.to_string(),
            display_name: None,
            preset: LuaPreset::Default,
            identity: LuaMember::new(&id),
            realms: Arc::new(RwLock::new(std::collections::HashMap::new())),
            running: Arc::new(RwLock::new(false)),
            correlation: None,
        }
    }

    pub fn with_preset(data_dir: &str, preset: LuaPreset) -> Self {
        let mut network = Self::new(data_dir);
        network.preset = preset;
        network
    }

    pub fn with_correlation(mut self, ctx: CorrelationContext) -> Self {
        self.correlation = Some(ctx);
        self
    }
}

impl UserData for LuaNetwork {
    fn add_fields<F: mlua::UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("data_dir", |_, this| Ok(this.data_dir.clone()));
        fields.add_field_method_get("display_name", |_, this| Ok(this.display_name.clone()));
        fields.add_field_method_get("preset", |_, this| Ok(this.preset));
        fields.add_field_method_get("identity", |_, this| Ok(this.identity.clone()));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // id() -> string (member ID)
        methods.add_method("id", |_, this, ()| Ok(this.identity.id.clone()));

        // is_running() -> bool
        methods.add_method("is_running", |_, this, ()| {
            Ok(*this.running.blocking_read())
        });

        // start() -> async
        methods.add_async_method("start", |_, this, ()| async move {
            let mut running = this.running.write().await;
            *running = true;
            if let Some(ref ctx) = this.correlation {
                tracing::info!(
                    trace_id = %ctx.trace_id_str(),
                    data_dir = %this.data_dir,
                    "Network started"
                );
            }
            Ok(())
        });

        // stop() -> async
        methods.add_async_method("stop", |_, this, ()| async move {
            let mut running = this.running.write().await;
            *running = false;
            if let Some(ref ctx) = this.correlation {
                tracing::info!(
                    trace_id = %ctx.trace_id_str(),
                    "Network stopped"
                );
            }
            Ok(())
        });

        // create_realm(name) -> Realm
        methods.add_async_method("create_realm", |_, this, name: String| async move {
            // Ensure network is started
            {
                let mut running = this.running.write().await;
                if !*running {
                    *running = true;
                }
            }

            let realm_id = generate_id();
            let mut realm = LuaRealm::new(&realm_id, Some(name.clone()));

            if let Some(ref ctx) = this.correlation {
                realm = realm.with_correlation(ctx.child());
                tracing::info!(
                    trace_id = %ctx.trace_id_str(),
                    realm_id = %realm_id,
                    name = %name,
                    "Realm created"
                );
            }

            this.realms.write().await.insert(realm_id.clone(), realm.clone());
            Ok(realm)
        });

        // join(invite_code) -> Realm
        methods.add_async_method("join", |_, this, invite: LuaInviteCode| async move {
            // Ensure network is started
            {
                let mut running = this.running.write().await;
                if !*running {
                    *running = true;
                }
            }

            let realm_id = invite.realm_id.clone();
            let mut realm = LuaRealm::new(&realm_id, None);

            if let Some(ref ctx) = this.correlation {
                realm = realm.with_correlation(ctx.child());
                tracing::info!(
                    trace_id = %ctx.trace_id_str(),
                    realm_id = %realm_id,
                    "Joined realm"
                );
            }

            this.realms.write().await.insert(realm_id.clone(), realm.clone());
            Ok(realm)
        });

        // realm(id) -> Realm or nil
        methods.add_method("realm", |_, this, id: String| {
            Ok(this.realms.blocking_read().get(&id).cloned())
        });

        // realms() -> [realm_id]
        methods.add_method("realms", |_, this, ()| {
            let realm_ids: Vec<String> = this.realms.blocking_read().keys().cloned().collect();
            Ok(realm_ids)
        });

        // set_display_name(name)
        methods.add_method_mut("set_display_name", |_, this, name: String| {
            this.display_name = Some(name.clone());
            this.identity.display_name = Some(name);
            Ok(())
        });

        // with_correlation(ctx) -> Network
        methods.add_method("with_correlation", |_, this, ctx: LuaCorrelationContext| {
            Ok(this.clone().with_correlation(ctx.0))
        });

        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            Ok(format!(
                "IndrasNetwork(data_dir=\"{}\", preset={:?}, running={})",
                this.data_dir,
                this.preset,
                *this.running.blocking_read()
            ))
        });
    }
}

// =============================================================================
// NetworkBuilder binding
// =============================================================================

/// Lua wrapper for NetworkBuilder
#[derive(Clone)]
pub struct LuaNetworkBuilder {
    data_dir: Option<String>,
    display_name: Option<String>,
    preset: LuaPreset,
    enforce_pq_signatures: bool,
}

impl Default for LuaNetworkBuilder {
    fn default() -> Self {
        Self {
            data_dir: None,
            display_name: None,
            preset: LuaPreset::Default,
            enforce_pq_signatures: false,
        }
    }
}

impl UserData for LuaNetworkBuilder {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // data_dir(path) -> self
        methods.add_method("data_dir", |_, this, path: String| {
            let mut builder = this.clone();
            builder.data_dir = Some(path);
            Ok(builder)
        });

        // display_name(name) -> self
        methods.add_method("display_name", |_, this, name: String| {
            let mut builder = this.clone();
            builder.display_name = Some(name);
            Ok(builder)
        });

        // preset(preset) -> self
        methods.add_method("preset", |_, this, preset: LuaPreset| {
            let mut builder = this.clone();
            builder.preset = preset;
            Ok(builder)
        });

        // enforce_pq_signatures() -> self
        methods.add_method("enforce_pq_signatures", |_, this, ()| {
            let mut builder = this.clone();
            builder.enforce_pq_signatures = true;
            Ok(builder)
        });

        // build() -> Network
        methods.add_async_method("build", |_, this, ()| async move {
            let data_dir = this.data_dir.clone().unwrap_or_else(|| "/tmp/indras".to_string());
            let mut network = LuaNetwork::with_preset(&data_dir, this.preset);
            network.display_name = this.display_name.clone();
            if let Some(ref name) = this.display_name {
                network.identity.display_name = Some(name.clone());
            }
            Ok(network)
        });

        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            Ok(format!(
                "NetworkBuilder(data_dir={:?}, preset={:?})",
                this.data_dir, this.preset
            ))
        });
    }
}

// =============================================================================
// Registration
// =============================================================================

/// Register SyncEngine types with the indras.sync_engine namespace
pub fn register(lua: &Lua, indras: &Table) -> Result<()> {
    let sync_engine = lua.create_table()?;

    // =================================
    // Preset constants
    // =================================
    let preset = lua.create_table()?;
    preset.set("Default", lua.create_function(|_, ()| Ok(LuaPreset::Default))?)?;
    preset.set("Chat", lua.create_function(|_, ()| Ok(LuaPreset::Chat))?)?;
    preset.set(
        "Collaboration",
        lua.create_function(|_, ()| Ok(LuaPreset::Collaboration))?,
    )?;
    preset.set("IoT", lua.create_function(|_, ()| Ok(LuaPreset::IoT))?)?;
    preset.set(
        "OfflineFirst",
        lua.create_function(|_, ()| Ok(LuaPreset::OfflineFirst))?,
    )?;
    sync_engine.set("Preset", preset)?;

    // =================================
    // Content constructors
    // =================================
    let content = lua.create_table()?;

    content.set(
        "text",
        lua.create_function(|_, text: String| Ok(LuaContent::Text(text)))?,
    )?;

    content.set(
        "binary",
        lua.create_function(|_, (mime_type, data): (String, mlua::String)| {
            Ok(LuaContent::Binary {
                mime_type,
                data: data.as_bytes().to_vec(),
            })
        })?,
    )?;

    content.set(
        "system",
        lua.create_function(|_, msg: String| Ok(LuaContent::System(msg)))?,
    )?;

    content.set(
        "json",
        lua.create_function(|lua, value: Value| {
            let json: serde_json::Value = lua.from_value(value)?;
            Ok(LuaContent::Json(json))
        })?,
    )?;

    sync_engine.set("Content", content)?;

    // =================================
    // InviteCode constructors
    // =================================
    let invite_code = lua.create_table()?;

    invite_code.set(
        "parse",
        lua.create_function(|_, code: String| LuaInviteCode::parse(&code))?,
    )?;

    invite_code.set(
        "new",
        lua.create_function(|_, realm_id: String| Ok(LuaInviteCode::new(&realm_id)))?,
    )?;

    sync_engine.set("InviteCode", invite_code)?;

    // =================================
    // Member constructors
    // =================================
    let member = lua.create_table()?;

    member.set(
        "new",
        lua.create_function(|_, id: String| Ok(LuaMember::new(&id)))?,
    )?;

    member.set(
        "with_name",
        lua.create_function(|_, (id, name): (String, String)| Ok(LuaMember::with_name(&id, &name)))?,
    )?;

    sync_engine.set("Member", member)?;

    // =================================
    // Network constructors
    // =================================
    let network = lua.create_table()?;

    // Network.new(data_dir) -> Network
    network.set(
        "new",
        lua.create_async_function(|_, data_dir: String| async move {
            Ok(LuaNetwork::new(&data_dir))
        })?,
    )?;

    // Network.preset(preset) -> NetworkBuilder
    network.set(
        "preset",
        lua.create_function(|_, preset: LuaPreset| {
            Ok(LuaNetworkBuilder {
                preset,
                ..Default::default()
            })
        })?,
    )?;

    // Network.builder() -> NetworkBuilder
    network.set(
        "builder",
        lua.create_function(|_, ()| Ok(LuaNetworkBuilder::default()))?,
    )?;

    sync_engine.set("Network", network)?;

    // =================================
    // Document constructor (for standalone use)
    // =================================
    let document = lua.create_table()?;

    document.set(
        "new",
        lua.create_function(|_, (name, realm_id): (String, String)| {
            Ok(LuaDocument::new(&name, &realm_id))
        })?,
    )?;

    sync_engine.set("Document", document)?;

    // =================================
    // Realm constructor (for standalone testing)
    // =================================
    let realm = lua.create_table()?;

    realm.set(
        "new",
        lua.create_function(|_, (id, name): (String, Option<String>)| Ok(LuaRealm::new(&id, name)))?,
    )?;

    sync_engine.set("Realm", realm)?;

    // =================================
    // Peer-based Realm utilities
    // =================================

    // compute_realm_id(peer_ids) -> realm_id string
    // Computes a deterministic realm ID from a set of peer IDs
    sync_engine.set(
        "compute_realm_id",
        lua.create_function(|_, peer_ids: Vec<String>| {
            if peer_ids.len() < 2 {
                return Err(mlua::Error::external(
                    "Peer-based realms require at least 2 peers",
                ));
            }
            Ok(compute_realm_id_for_peers(&peer_ids))
        })?,
    )?;

    // normalize_peers(peer_ids) -> sorted, deduped peer_ids
    sync_engine.set(
        "normalize_peers",
        lua.create_function(|_, peer_ids: Vec<String>| Ok(normalize_peers(&peer_ids)))?,
    )?;

    // =================================
    // Quest constructors and operations
    // =================================
    let quest = lua.create_table()?;

    // Quest.new(title, description, creator) -> Quest
    quest.set(
        "new",
        lua.create_function(
            |_, (title, description, image, creator): (String, String, Option<String>, String)| {
                Ok(LuaQuest::new(&title, &description, image, &creator))
            },
        )?,
    )?;

    // Quest.create(realm_id, title, description, creator) -> Quest
    // Creates a quest and returns it (for simulation, realm_id is tracked externally)
    quest.set(
        "create",
        lua.create_function(
            |_,
             (realm_id, title, description, creator): (
                String,
                String,
                String,
                String,
            )| {
                let mut quest = LuaQuest::new(&title, &description, None, &creator);
                // Include realm_id in quest id for tracking
                quest.id = format!("{}:{}", &realm_id[..8.min(realm_id.len())], quest.id);
                Ok(quest)
            },
        )?,
    )?;

    sync_engine.set("quest", quest)?;

    // =================================
    // QuestClaim constructor
    // =================================
    let quest_claim = lua.create_table()?;

    quest_claim.set(
        "new",
        lua.create_function(|_, (claimant, proof): (String, Option<String>)| {
            Ok(LuaQuestClaim::new(&claimant, proof))
        })?,
    )?;

    sync_engine.set("QuestClaim", quest_claim)?;

    // =================================
    // Contacts operations
    // =================================
    let contacts = lua.create_table()?;

    // Contacts.new() -> Contacts
    contacts.set("new", lua.create_function(|_, ()| Ok(LuaContacts::new()))?)?;

    sync_engine.set("contacts", contacts)?;

    // =================================
    // Attention tracking operations
    // =================================
    let attention = lua.create_table()?;

    // Attention.new() -> AttentionDocument
    attention.set("new", lua.create_function(|_, ()| Ok(LuaAttentionDocument::new()))?)?;

    sync_engine.set("attention", attention)?;

    // Set sync_engine namespace on indras
    indras.set("sync_engine", sync_engine)?;

    Ok(())
}

// =============================================================================
// Utility functions
// =============================================================================

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len])
    }
}

fn generate_id() -> String {
    uuid::Uuid::new_v4().to_string().replace("-", "")
}

fn current_timestamp() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn base64_encode(data: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(data)
}

fn base64_decode(s: &str) -> std::result::Result<Vec<u8>, base64::DecodeError> {
    URL_SAFE_NO_PAD.decode(s)
}

/// Compute a deterministic realm ID from a set of peer IDs.
///
/// This uses a deterministic hash approach:
/// - Sort peer IDs deterministically
/// - Concatenate with prefix and compute a simple hash
/// - Return as hex string
///
/// For simulation purposes, this provides a deterministic mapping
/// without requiring the exact same hash as the Rust implementation.
fn compute_realm_id_for_peers(peer_ids: &[String]) -> String {
    use std::collections::BTreeSet;

    // Sort and dedupe peer IDs
    let sorted: BTreeSet<&String> = peer_ids.iter().collect();

    // Use a deterministic hasher (FNV-like approach)
    let mut hash: u64 = 0xcbf29ce484222325; // FNV offset basis
    let fnv_prime: u64 = 0x100000001b3;

    // Hash the prefix
    for byte in b"realm-peers-v1:" {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(fnv_prime);
    }

    // Hash each peer ID
    for peer_id in sorted {
        for byte in peer_id.as_bytes() {
            hash ^= *byte as u64;
            hash = hash.wrapping_mul(fnv_prime);
        }
        // Separator between peer IDs
        hash ^= 0xFF;
        hash = hash.wrapping_mul(fnv_prime);
    }

    // Generate a 32-character hex string (16 bytes)
    let hash2 = hash.wrapping_mul(fnv_prime);
    format!("{:016x}{:016x}", hash, hash2)
}

/// Normalize a list of peer IDs (sort and dedupe).
fn normalize_peers(peer_ids: &[String]) -> Vec<String> {
    use std::collections::BTreeSet;
    let sorted: BTreeSet<String> = peer_ids.iter().cloned().collect();
    sorted.into_iter().collect()
}

// =============================================================================
// Quest and QuestClaim bindings
// =============================================================================

/// Lua wrapper for QuestClaim
#[derive(Debug, Clone)]
pub struct LuaQuestClaim {
    pub claimant: String,
    pub proof: Option<String>,
    pub submitted_at_millis: i64,
    pub verified: bool,
    pub verified_at_millis: Option<i64>,
}

impl LuaQuestClaim {
    pub fn new(claimant: &str, proof: Option<String>) -> Self {
        Self {
            claimant: claimant.to_string(),
            proof,
            submitted_at_millis: current_timestamp(),
            verified: false,
            verified_at_millis: None,
        }
    }

    pub fn verify(&mut self) {
        if !self.verified {
            self.verified = true;
            self.verified_at_millis = Some(current_timestamp());
        }
    }
}

impl UserData for LuaQuestClaim {
    fn add_fields<F: mlua::UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("claimant", |_, this| Ok(this.claimant.clone()));
        fields.add_field_method_get("proof", |_, this| Ok(this.proof.clone()));
        fields.add_field_method_get("submitted_at_millis", |_, this| Ok(this.submitted_at_millis));
        fields.add_field_method_get("verified", |_, this| Ok(this.verified));
        fields.add_field_method_get("verified_at_millis", |_, this| Ok(this.verified_at_millis));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("is_verified", |_, this, ()| Ok(this.verified));

        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            Ok(format!(
                "QuestClaim(claimant={}, verified={})",
                truncate(&this.claimant, 8),
                this.verified
            ))
        });
    }
}

/// Lua wrapper for Quest
#[derive(Clone)]
pub struct LuaQuest {
    pub id: String,
    pub title: String,
    pub description: String,
    pub image: Option<String>,
    pub creator: String,
    pub claims: Arc<RwLock<Vec<LuaQuestClaim>>>,
    pub created_at_millis: i64,
    pub completed_at_millis: Option<i64>,
}

impl LuaQuest {
    pub fn new(title: &str, description: &str, image: Option<String>, creator: &str) -> Self {
        Self {
            id: generate_id(),
            title: title.to_string(),
            description: description.to_string(),
            image,
            creator: creator.to_string(),
            claims: Arc::new(RwLock::new(Vec::new())),
            created_at_millis: current_timestamp(),
            completed_at_millis: None,
        }
    }
}

impl UserData for LuaQuest {
    fn add_fields<F: mlua::UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("id", |_, this| Ok(this.id.clone()));
        fields.add_field_method_get("title", |_, this| Ok(this.title.clone()));
        fields.add_field_method_get("description", |_, this| Ok(this.description.clone()));
        fields.add_field_method_get("image", |_, this| Ok(this.image.clone()));
        fields.add_field_method_get("creator", |_, this| Ok(this.creator.clone()));
        fields.add_field_method_get("created_at_millis", |_, this| Ok(this.created_at_millis));
        fields.add_field_method_get("completed_at_millis", |_, this| Ok(this.completed_at_millis));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // has_claims() -> bool (async for compatibility)
        methods.add_async_method("has_claims", |_, this, ()| async move {
            Ok(!this.claims.read().await.is_empty())
        });

        // has_verified_claims() -> bool (async for compatibility)
        methods.add_async_method("has_verified_claims", |_, this, ()| async move {
            Ok(this.claims.read().await.iter().any(|c| c.verified))
        });

        // is_complete() -> bool
        methods.add_method("is_complete", |_, this, ()| {
            Ok(this.completed_at_millis.is_some())
        });

        // is_open() -> bool (async for compatibility)
        methods.add_async_method("is_open", |_, this, ()| async move {
            Ok(this.claims.read().await.is_empty() && this.completed_at_millis.is_none())
        });

        // claim_count() -> number (async for compatibility)
        methods.add_async_method("claim_count", |_, this, ()| async move {
            Ok(this.claims.read().await.len())
        });

        // get_claim(index) -> QuestClaim or nil (async for compatibility)
        methods.add_async_method("get_claim", |_, this, index: usize| async move {
            Ok(this.claims.read().await.get(index).cloned())
        });

        // pending_claims() -> [QuestClaim] (async for compatibility)
        methods.add_async_method("pending_claims", |_, this, ()| async move {
            let claims: Vec<LuaQuestClaim> = this
                .claims
                .read()
                .await
                .iter()
                .filter(|c| !c.verified)
                .cloned()
                .collect();
            Ok(claims)
        });

        // verified_claims() -> [QuestClaim] (async for compatibility)
        methods.add_async_method("verified_claims", |_, this, ()| async move {
            let claims: Vec<LuaQuestClaim> = this
                .claims
                .read()
                .await
                .iter()
                .filter(|c| c.verified)
                .cloned()
                .collect();
            Ok(claims)
        });

        // submit_claim(claimant, proof) -> claim_index
        methods.add_async_method(
            "submit_claim",
            |_, this, (claimant, proof): (String, Option<String>)| async move {
                if this.completed_at_millis.is_some() {
                    return Err(mlua::Error::external("Quest is already complete"));
                }
                let claim = LuaQuestClaim::new(&claimant, proof);
                let mut claims = this.claims.write().await;
                claims.push(claim);
                Ok(claims.len() - 1)
            },
        );

        // verify_claim(claim_index)
        methods.add_async_method("verify_claim", |_, this, claim_index: usize| async move {
            let mut claims = this.claims.write().await;
            if claim_index >= claims.len() {
                return Err(mlua::Error::external("Claim not found"));
            }
            claims[claim_index].verify();
            Ok(())
        });

        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            // For toString, we need a sync version - use try_read with fallback
            let claim_count = this.claims.try_read().map(|c| c.len()).unwrap_or(0);
            Ok(format!(
                "Quest(id={}, title=\"{}\", claims={})",
                truncate(&this.id, 8),
                truncate(&this.title, 20),
                claim_count
            ))
        });
    }
}

// =============================================================================
// Contacts binding
// =============================================================================

/// Lua wrapper for contacts management
#[derive(Clone)]
pub struct LuaContacts {
    /// Set of contact member IDs
    contacts: Arc<RwLock<std::collections::HashSet<String>>>,
}

impl LuaContacts {
    pub fn new() -> Self {
        Self {
            contacts: Arc::new(RwLock::new(std::collections::HashSet::new())),
        }
    }
}

impl Default for LuaContacts {
    fn default() -> Self {
        Self::new()
    }
}

impl UserData for LuaContacts {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // add(member_id) -> bool (true if newly added)
        methods.add_async_method("add", |_, this, member_id: String| async move {
            Ok(this.contacts.write().await.insert(member_id))
        });

        // remove(member_id) -> bool (true if was present)
        methods.add_async_method("remove", |_, this, member_id: String| async move {
            Ok(this.contacts.write().await.remove(&member_id))
        });

        // contains(member_id) -> bool (async for compatibility)
        methods.add_async_method("contains", |_, this, member_id: String| async move {
            Ok(this.contacts.read().await.contains(&member_id))
        });

        // list() -> [member_id] (async for compatibility)
        methods.add_async_method("list", |_, this, ()| async move {
            let contacts: Vec<String> = this.contacts.read().await.iter().cloned().collect();
            Ok(contacts)
        });

        // count() -> number (async for compatibility)
        methods.add_async_method("count", |_, this, ()| async move {
            Ok(this.contacts.read().await.len())
        });

        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            // For toString, use try_read with fallback
            let count = this.contacts.try_read().map(|c| c.len()).unwrap_or(0);
            Ok(format!("Contacts(count={})", count))
        });
    }
}

// =============================================================================
// Attention tracking binding
// =============================================================================

/// Attention switch event in the attention log.
#[derive(Debug, Clone)]
pub struct LuaAttentionEvent {
    pub event_id: String,
    pub member: String,
    pub quest_id: Option<String>,
    pub timestamp_millis: i64,
}

impl LuaAttentionEvent {
    pub fn new(member: &str, quest_id: Option<String>) -> Self {
        Self {
            event_id: generate_id(),
            member: member.to_string(),
            quest_id,
            timestamp_millis: current_timestamp(),
        }
    }
}

impl UserData for LuaAttentionEvent {
    fn add_fields<F: mlua::UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("event_id", |_, this| Ok(this.event_id.clone()));
        fields.add_field_method_get("member", |_, this| Ok(this.member.clone()));
        fields.add_field_method_get("quest_id", |_, this| Ok(this.quest_id.clone()));
        fields.add_field_method_get("timestamp_millis", |_, this| Ok(this.timestamp_millis));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            Ok(format!(
                "AttentionEvent(member={}, quest={})",
                truncate(&this.member, 8),
                this.quest_id.as_deref().unwrap_or("none")
            ))
        });
    }
}

/// Computed attention value for a quest.
#[derive(Debug, Clone, Default)]
pub struct LuaQuestAttention {
    pub quest_id: String,
    pub total_attention_millis: u64,
    pub attention_by_member: std::collections::HashMap<String, u64>,
    pub currently_focused_members: Vec<String>,
}

impl UserData for LuaQuestAttention {
    fn add_fields<F: mlua::UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("quest_id", |_, this| Ok(this.quest_id.clone()));
        fields.add_field_method_get("total_attention_millis", |_, this| {
            Ok(this.total_attention_millis)
        });
        fields.add_field_method_get("currently_focused_members", |_, this| {
            Ok(this.currently_focused_members.clone())
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("attention_by_member", |_, this, member: String| {
            Ok(this.attention_by_member.get(&member).copied().unwrap_or(0))
        });

        methods.add_method("total_attention_secs", |_, this, ()| {
            Ok(this.total_attention_millis as f64 / 1000.0)
        });
    }
}

/// Lua wrapper for attention tracking document.
#[derive(Clone)]
pub struct LuaAttentionDocument {
    /// Append-only log of attention events
    events: Arc<RwLock<Vec<LuaAttentionEvent>>>,
    /// Current focus per member
    current_focus: Arc<RwLock<std::collections::HashMap<String, Option<String>>>>,
}

impl LuaAttentionDocument {
    pub fn new() -> Self {
        Self {
            events: Arc::new(RwLock::new(Vec::new())),
            current_focus: Arc::new(RwLock::new(std::collections::HashMap::new())),
        }
    }
}

impl Default for LuaAttentionDocument {
    fn default() -> Self {
        Self::new()
    }
}

impl UserData for LuaAttentionDocument {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // focus_on_quest(member, quest_id) -> event_id
        methods.add_async_method(
            "focus_on_quest",
            |_, this, (member, quest_id): (String, String)| async move {
                let event = LuaAttentionEvent::new(&member, Some(quest_id.clone()));
                let event_id = event.event_id.clone();

                this.events.write().await.push(event);
                this.current_focus
                    .write()
                    .await
                    .insert(member, Some(quest_id));

                Ok(event_id)
            },
        );

        // clear_attention(member) -> event_id
        methods.add_async_method("clear_attention", |_, this, member: String| async move {
            let event = LuaAttentionEvent::new(&member, None);
            let event_id = event.event_id.clone();

            this.events.write().await.push(event);
            this.current_focus.write().await.insert(member, None);

            Ok(event_id)
        });

        // current_focus(member) -> quest_id or nil
        methods.add_async_method("current_focus", |_, this, member: String| async move {
            Ok(this
                .current_focus
                .read()
                .await
                .get(&member)
                .cloned()
                .flatten())
        });

        // members_focusing_on(quest_id) -> [member]
        methods.add_async_method(
            "members_focusing_on",
            |_, this, quest_id: String| async move {
                let focusers: Vec<String> = this
                    .current_focus
                    .read()
                    .await
                    .iter()
                    .filter_map(|(member, focus)| {
                        if focus.as_ref() == Some(&quest_id) {
                            Some(member.clone())
                        } else {
                            None
                        }
                    })
                    .collect();
                Ok(focusers)
            },
        );

        // event_count() -> number
        methods.add_async_method("event_count", |_, this, ()| async move {
            Ok(this.events.read().await.len())
        });

        // calculate_attention(as_of_millis) -> [QuestAttention]
        methods.add_async_method(
            "calculate_attention",
            |_, this, as_of: Option<i64>| async move {
                let as_of = as_of.unwrap_or_else(current_timestamp);
                let events = this.events.read().await;

                // Track attention windows per member
                let mut member_windows: std::collections::HashMap<String, (String, i64)> =
                    std::collections::HashMap::new();

                // Accumulated attention per quest per member
                let mut quest_attention: std::collections::HashMap<
                    String,
                    std::collections::HashMap<String, u64>,
                > = std::collections::HashMap::new();

                // Sort events by timestamp
                let mut sorted_events: Vec<_> = events.iter().collect();
                sorted_events.sort_by_key(|e| e.timestamp_millis);

                for event in sorted_events {
                    if event.timestamp_millis > as_of {
                        continue;
                    }

                    // Close any open window for this member
                    if let Some((prev_quest, start_time)) = member_windows.remove(&event.member) {
                        let duration = (event.timestamp_millis - start_time).max(0) as u64;
                        *quest_attention
                            .entry(prev_quest)
                            .or_default()
                            .entry(event.member.clone())
                            .or_default() += duration;
                    }

                    // Open new window if focusing on a quest
                    if let Some(ref quest_id) = event.quest_id {
                        member_windows
                            .insert(event.member.clone(), (quest_id.clone(), event.timestamp_millis));
                    }
                }

                // Close open windows at as_of time
                for (member, (quest_id, start_time)) in &member_windows {
                    let duration = (as_of - start_time).max(0) as u64;
                    *quest_attention
                        .entry(quest_id.clone())
                        .or_default()
                        .entry(member.clone())
                        .or_default() += duration;
                }

                // Build result
                let mut result: Vec<LuaQuestAttention> = quest_attention
                    .into_iter()
                    .map(|(quest_id, by_member)| {
                        let total_attention_millis: u64 = by_member.values().sum();
                        let currently_focused_members: Vec<String> = member_windows
                            .iter()
                            .filter_map(|(m, (q, _))| {
                                if q == &quest_id {
                                    Some(m.clone())
                                } else {
                                    None
                                }
                            })
                            .collect();

                        LuaQuestAttention {
                            quest_id,
                            total_attention_millis,
                            attention_by_member: by_member,
                            currently_focused_members,
                        }
                    })
                    .collect();

                // Sort by total attention (highest first), then by quest_id for stable ordering
                result.sort_by(|a, b| {
                    match b.total_attention_millis.cmp(&a.total_attention_millis) {
                        std::cmp::Ordering::Equal => a.quest_id.cmp(&b.quest_id),
                        other => other,
                    }
                });

                Ok(result)
            },
        );

        // quests_by_attention() -> [QuestAttention] (sorted by attention, highest first)
        methods.add_async_method("quests_by_attention", |_, this, ()| async move {
            // Delegate to calculate_attention with current time
            let as_of = current_timestamp();
            let events = this.events.read().await;

            let mut member_windows: std::collections::HashMap<String, (String, i64)> =
                std::collections::HashMap::new();
            let mut quest_attention: std::collections::HashMap<
                String,
                std::collections::HashMap<String, u64>,
            > = std::collections::HashMap::new();

            let mut sorted_events: Vec<_> = events.iter().collect();
            sorted_events.sort_by_key(|e| e.timestamp_millis);

            for event in sorted_events {
                if let Some((prev_quest, start_time)) = member_windows.remove(&event.member) {
                    let duration = (event.timestamp_millis - start_time).max(0) as u64;
                    *quest_attention
                        .entry(prev_quest)
                        .or_default()
                        .entry(event.member.clone())
                        .or_default() += duration;
                }
                if let Some(ref quest_id) = event.quest_id {
                    member_windows
                        .insert(event.member.clone(), (quest_id.clone(), event.timestamp_millis));
                }
            }

            for (member, (quest_id, start_time)) in &member_windows {
                let duration = (as_of - start_time).max(0) as u64;
                *quest_attention
                    .entry(quest_id.clone())
                    .or_default()
                    .entry(member.clone())
                    .or_default() += duration;
            }

            let mut result: Vec<LuaQuestAttention> = quest_attention
                .into_iter()
                .map(|(quest_id, by_member)| {
                    let total_attention_millis: u64 = by_member.values().sum();
                    let currently_focused_members: Vec<String> = member_windows
                        .iter()
                        .filter_map(|(m, (q, _))| {
                            if q == &quest_id {
                                Some(m.clone())
                            } else {
                                None
                            }
                        })
                        .collect();

                    LuaQuestAttention {
                        quest_id,
                        total_attention_millis,
                        attention_by_member: by_member,
                        currently_focused_members,
                    }
                })
                .collect();

            // Sort by attention (descending), then by quest_id (ascending) for stable ordering
            result.sort_by(|a, b| {
                match b.total_attention_millis.cmp(&a.total_attention_millis) {
                    std::cmp::Ordering::Equal => a.quest_id.cmp(&b.quest_id),
                    other => other,
                }
            });
            Ok(result)
        });

        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            let count = this.events.try_read().map(|e| e.len()).unwrap_or(0);
            Ok(format!("AttentionDocument(events={})", count))
        });
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_lua() -> Lua {
        let lua = Lua::new();
        let indras = lua.create_table().unwrap();
        super::super::correlation::register(&lua, &indras).unwrap();
        register(&lua, &indras).unwrap();
        lua.globals().set("indras", indras).unwrap();
        lua
    }

    #[test]
    fn test_preset_constants() {
        let lua = setup_lua();

        let result: String = lua
            .load(
                r#"
                local preset = indras.sync_engine.Preset.Chat()
                return preset.name
            "#,
            )
            .eval()
            .unwrap();
        assert_eq!(result, "chat");
    }

    #[test]
    fn test_preset_properties() {
        let lua = setup_lua();

        let (capacity, interval): (i32, i32) = lua
            .load(
                r#"
                local preset = indras.sync_engine.Preset.IoT()
                return preset.event_channel_capacity, preset.sync_interval_secs
            "#,
            )
            .eval()
            .unwrap();
        assert_eq!(capacity, 256);
        assert_eq!(interval, 30);
    }

    #[test]
    fn test_content_text() {
        let lua = setup_lua();

        let result: String = lua
            .load(
                r#"
                local content = indras.sync_engine.Content.text("Hello, world!")
                return content:as_text()
            "#,
            )
            .eval()
            .unwrap();
        assert_eq!(result, "Hello, world!");
    }

    #[test]
    fn test_content_types() {
        let lua = setup_lua();

        let (is_text, is_binary): (bool, bool) = lua
            .load(
                r#"
                local text = indras.sync_engine.Content.text("Hello")
                local binary = indras.sync_engine.Content.binary("image/png", "data")
                return text:is_text(), binary:is_binary()
            "#,
            )
            .eval()
            .unwrap();
        assert!(is_text);
        assert!(is_binary);
    }

    #[test]
    fn test_invite_code() {
        let lua = setup_lua();

        let result: bool = lua
            .load(
                r#"
                local invite = indras.sync_engine.InviteCode.new("testrealm123")
                local uri = invite:to_uri()
                return uri:find("indra:realm:") ~= nil
            "#,
            )
            .eval()
            .unwrap();
        assert!(result);
    }

    #[test]
    fn test_member() {
        let lua = setup_lua();

        let (name, short_id): (String, String) = lua
            .load(
                r#"
                local member = indras.sync_engine.Member.with_name("abc123def456", "Alice")
                return member:name(), member.short_id
            "#,
            )
            .eval()
            .unwrap();
        assert_eq!(name, "Alice");
        assert_eq!(short_id, "abc123de");
    }

    #[tokio::test]
    async fn test_network_creation() {
        let lua = setup_lua();

        let result: bool = lua
            .load(
                r#"
                local network = indras.sync_engine.Network.new("/tmp/test")
                return network.data_dir == "/tmp/test"
            "#,
            )
            .eval_async()
            .await
            .unwrap();
        assert!(result);
    }

    #[tokio::test]
    async fn test_network_builder() {
        let lua = setup_lua();

        let result: (String, String) = lua
            .load(
                r#"
                local network = indras.sync_engine.Network.builder()
                    :data_dir("/tmp/myapp")
                    :display_name("TestNode")
                    :preset(indras.sync_engine.Preset.Chat())
                    :build()
                return network.data_dir, network.display_name
            "#,
            )
            .eval_async()
            .await
            .unwrap();
        assert_eq!(result.0, "/tmp/myapp");
        assert_eq!(result.1, "TestNode");
    }

    #[tokio::test]
    async fn test_realm_creation() {
        let lua = setup_lua();

        let result: (bool, bool) = lua
            .load(
                r#"
                local network = indras.sync_engine.Network.new("/tmp/test")
                local realm = network:create_realm("My Project")
                return realm.name == "My Project", realm.invite_code ~= nil
            "#,
            )
            .eval_async()
            .await
            .unwrap();
        assert!(result.0);
        assert!(result.1);
    }

    #[tokio::test]
    async fn test_realm_messaging() {
        let lua = setup_lua();

        let count: i32 = lua
            .load(
                r#"
                local network = indras.sync_engine.Network.new("/tmp/test")
                local realm = network:create_realm("Chat Room")

                realm:send("Hello!")
                realm:send("World!")

                return realm:message_count()
            "#,
            )
            .eval_async()
            .await
            .unwrap();
        assert_eq!(count, 2);
    }

    #[tokio::test]
    async fn test_document_operations() {
        let lua = setup_lua();

        let result: (String, i32) = lua
            .load(
                r#"
                local network = indras.sync_engine.Network.new("/tmp/test")
                local realm = network:create_realm("Collab")
                local doc = realm:document("tasks")

                doc:set("title", "My Tasks")
                doc:set("count", 5)

                local state = doc:read_async()
                return state.title, state.count
            "#,
            )
            .eval_async()
            .await
            .unwrap();
        assert_eq!(result.0, "My Tasks");
        assert_eq!(result.1, 5);
    }

    #[tokio::test]
    async fn test_correlation_context_integration() {
        let lua = setup_lua();

        let result: bool = lua
            .load(
                r#"
                local ctx = indras.correlation.new_root()
                local network = indras.sync_engine.Network.new("/tmp/test")
                    :with_correlation(ctx)
                return network ~= nil
            "#,
            )
            .eval_async()
            .await
            .unwrap();
        assert!(result);
    }

    #[tokio::test]
    async fn test_join_realm() {
        let lua = setup_lua();

        let result: bool = lua
            .load(
                r#"
                local network = indras.sync_engine.Network.new("/tmp/test")
                local realm1 = network:create_realm("Source")
                local invite = realm1.invite_code

                local network2 = indras.sync_engine.Network.new("/tmp/test2")
                local realm2 = network2:join(invite)

                return realm2.id == realm1.id
            "#,
            )
            .eval_async()
            .await
            .unwrap();
        assert!(result);
    }

    // =================================
    // Peer-based Realm Tests
    // =================================

    #[test]
    fn test_compute_realm_id_deterministic() {
        let lua = setup_lua();

        let result: bool = lua
            .load(
                r#"
                -- Same peer set in different order should produce same realm ID
                local realm1 = indras.sync_engine.compute_realm_id({"alice", "bob", "charlie"})
                local realm2 = indras.sync_engine.compute_realm_id({"charlie", "alice", "bob"})
                local realm3 = indras.sync_engine.compute_realm_id({"bob", "charlie", "alice"})
                return realm1 == realm2 and realm2 == realm3
            "#,
            )
            .eval()
            .unwrap();
        assert!(result);
    }

    #[test]
    fn test_compute_realm_id_uniqueness() {
        let lua = setup_lua();

        let result: bool = lua
            .load(
                r#"
                -- Different peer sets should produce different realm IDs
                local realm1 = indras.sync_engine.compute_realm_id({"alice", "bob"})
                local realm2 = indras.sync_engine.compute_realm_id({"alice", "charlie"})
                local realm3 = indras.sync_engine.compute_realm_id({"bob", "charlie"})
                return realm1 ~= realm2 and realm2 ~= realm3 and realm1 ~= realm3
            "#,
            )
            .eval()
            .unwrap();
        assert!(result);
    }

    #[test]
    fn test_compute_realm_id_min_peers() {
        let lua = setup_lua();

        // Should fail with less than 2 peers
        let result = lua
            .load(
                r#"
                indras.sync_engine.compute_realm_id({"alice"})
            "#,
            )
            .eval::<String>();
        assert!(result.is_err());
    }

    #[test]
    fn test_normalize_peers() {
        let lua = setup_lua();

        let result: Vec<String> = lua
            .load(
                r#"
                return indras.sync_engine.normalize_peers({"charlie", "alice", "bob", "alice"})
            "#,
            )
            .eval()
            .unwrap();
        assert_eq!(result, vec!["alice", "bob", "charlie"]);
    }

    // =================================
    // Quest Tests
    // =================================

    #[test]
    fn test_quest_creation() {
        let lua = setup_lua();

        let (title, creator, is_open): (String, String, bool) = lua
            .load(
                r#"
                local quest = indras.sync_engine.quest.new(
                    "Review design doc",
                    "Please review the attached PDF",
                    nil,
                    "alice123"
                )
                return quest.title, quest.creator, quest:is_open()
            "#,
            )
            .eval()
            .unwrap();
        assert_eq!(title, "Review design doc");
        assert_eq!(creator, "alice123");
        assert!(is_open);
    }

    #[tokio::test]
    async fn test_quest_submit_claim() {
        let lua = setup_lua();

        let (claim_count, has_claims, is_open): (usize, bool, bool) = lua
            .load(
                r#"
                local quest = indras.sync_engine.quest.new(
                    "Review design doc",
                    "Please review the attached PDF",
                    nil,
                    "alice123"
                )

                -- Submit a claim
                quest:submit_claim("bob456", "proof_artifact_id")

                return quest:claim_count(), quest:has_claims(), quest:is_open()
            "#,
            )
            .eval_async()
            .await
            .unwrap();
        assert_eq!(claim_count, 1);
        assert!(has_claims);
        assert!(!is_open);
    }

    #[tokio::test]
    async fn test_quest_verify_claim() {
        let lua = setup_lua();

        let (pending_before, verified_before, pending_after, verified_after): (
            usize,
            usize,
            usize,
            usize,
        ) = lua
            .load(
                r#"
                local quest = indras.sync_engine.quest.new(
                    "Review design doc",
                    "Please review the attached PDF",
                    nil,
                    "alice123"
                )

                quest:submit_claim("bob456", "proof1")
                quest:submit_claim("charlie789", "proof2")

                local pending_before = #quest:pending_claims()
                local verified_before = #quest:verified_claims()

                -- Verify first claim
                quest:verify_claim(0)

                local pending_after = #quest:pending_claims()
                local verified_after = #quest:verified_claims()

                return pending_before, verified_before, pending_after, verified_after
            "#,
            )
            .eval_async()
            .await
            .unwrap();
        assert_eq!(pending_before, 2);
        assert_eq!(verified_before, 0);
        assert_eq!(pending_after, 1);
        assert_eq!(verified_after, 1);
    }

    // =================================
    // Contacts Tests
    // =================================

    #[tokio::test]
    async fn test_contacts_operations() {
        let lua = setup_lua();

        let (count_after_add, contains, count_after_remove): (usize, bool, usize) = lua
            .load(
                r#"
                local contacts = indras.sync_engine.contacts.new()

                contacts:add("alice123")
                contacts:add("bob456")
                contacts:add("charlie789")

                local count_after_add = contacts:count()
                local contains = contacts:contains("bob456")

                contacts:remove("bob456")

                local count_after_remove = contacts:count()

                return count_after_add, contains, count_after_remove
            "#,
            )
            .eval_async()
            .await
            .unwrap();
        assert_eq!(count_after_add, 3);
        assert!(contains);
        assert_eq!(count_after_remove, 2);
    }

    #[tokio::test]
    async fn test_contacts_list() {
        let lua = setup_lua();

        let contacts: Vec<String> = lua
            .load(
                r#"
                local contacts = indras.sync_engine.contacts.new()
                contacts:add("bob")
                contacts:add("alice")
                return contacts:list()
            "#,
            )
            .eval_async()
            .await
            .unwrap();
        assert_eq!(contacts.len(), 2);
        assert!(contacts.contains(&"alice".to_string()));
        assert!(contacts.contains(&"bob".to_string()));
    }
}
