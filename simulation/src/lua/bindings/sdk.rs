//! Lua bindings for indras-network SDK
//!
//! Provides Lua wrappers for the high-level SDK types:
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
// simulate the SDK behavior without actual network operations.

/// Configuration preset enum for SDK
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

/// Lua wrapper for IndrasNetwork (main SDK entry point)
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

/// Register SDK types with the indras.sdk namespace
pub fn register(lua: &Lua, indras: &Table) -> Result<()> {
    let sdk = lua.create_table()?;

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
    sdk.set("Preset", preset)?;

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

    sdk.set("Content", content)?;

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

    sdk.set("InviteCode", invite_code)?;

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

    sdk.set("Member", member)?;

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

    sdk.set("Network", network)?;

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

    sdk.set("Document", document)?;

    // =================================
    // Realm constructor (for standalone testing)
    // =================================
    let realm = lua.create_table()?;

    realm.set(
        "new",
        lua.create_function(|_, (id, name): (String, Option<String>)| Ok(LuaRealm::new(&id, name)))?,
    )?;

    sdk.set("Realm", realm)?;

    // Set sdk namespace on indras
    indras.set("sdk", sdk)?;

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
                local preset = indras.sdk.Preset.Chat()
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
                local preset = indras.sdk.Preset.IoT()
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
                local content = indras.sdk.Content.text("Hello, world!")
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
                local text = indras.sdk.Content.text("Hello")
                local binary = indras.sdk.Content.binary("image/png", "data")
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
                local invite = indras.sdk.InviteCode.new("testrealm123")
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
                local member = indras.sdk.Member.with_name("abc123def456", "Alice")
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
                local network = indras.sdk.Network.new("/tmp/test")
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
                local network = indras.sdk.Network.builder()
                    :data_dir("/tmp/myapp")
                    :display_name("TestNode")
                    :preset(indras.sdk.Preset.Chat())
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
                local network = indras.sdk.Network.new("/tmp/test")
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
                local network = indras.sdk.Network.new("/tmp/test")
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
                local network = indras.sdk.Network.new("/tmp/test")
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
                local network = indras.sdk.Network.new("/tmp/test")
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
                local network = indras.sdk.Network.new("/tmp/test")
                local realm1 = network:create_realm("Source")
                local invite = realm1.invite_code

                local network2 = indras.sdk.Network.new("/tmp/test2")
                local realm2 = network2:join(invite)

                return realm2.id == realm1.id
            "#,
            )
            .eval_async()
            .await
            .unwrap();
        assert!(result);
    }
}
