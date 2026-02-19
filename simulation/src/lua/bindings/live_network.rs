//! Lua bindings for the high-level IndrasNetwork API.
//!
//! These bindings wrap `indras_network` types (IndrasNetwork, Realm, Document,
//! HomeRealm, ContactsRealm) and expose them to Lua scenarios for integration
//! testing of the full network stack.

use base64::{engine::general_purpose::STANDARD, Engine as _};
use mlua::{Lua, LuaSerdeExt, MetaMethod, Result, Table, UserData, UserDataMethods, Value};
use std::sync::Arc;

use indras_network::{
    AccessMode, ArtifactId, ContactsRealm, Content, Document, DocumentSchema, HomeRealm,
    IndrasNetwork, MemberId, Message, MessageId, Realm, RealmChatDocument, RealmId,
};

// ============================================================
// LuaJsonDoc — newtype for Document<T> usage from Lua
// ============================================================

/// A JSON-backed document schema for use from Lua.
///
/// Deep-merges object keys; otherwise overwrites.
///
/// Custom Serialize/Deserialize encodes the inner `serde_json::Value` as a
/// JSON string so that postcard (which doesn't support `deserialize_any`)
/// can round-trip it reliably.
#[derive(Default, Clone)]
struct LuaJsonDoc(serde_json::Value);

impl serde::Serialize for LuaJsonDoc {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        let json = serde_json::to_string(&self.0).map_err(serde::ser::Error::custom)?;
        serializer.serialize_str(&json)
    }
}

impl<'de> serde::Deserialize<'de> for LuaJsonDoc {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let json = <String as serde::Deserialize>::deserialize(deserializer)?;
        let val = serde_json::from_str(&json).map_err(serde::de::Error::custom)?;
        Ok(LuaJsonDoc(val))
    }
}

impl DocumentSchema for LuaJsonDoc {
    fn merge(&mut self, remote: Self) {
        match (&mut self.0, remote.0) {
            (serde_json::Value::Object(a), serde_json::Value::Object(b)) => {
                for (k, v) in b {
                    a.insert(k, v);
                }
            }
            (_, other) => {
                self.0 = other;
            }
        }
    }
}

// ============================================================
// Helper functions
// ============================================================

fn parse_member_id(hex_str: &str) -> std::result::Result<MemberId, mlua::Error> {
    let bytes = hex::decode(hex_str)
        .map_err(|e| mlua::Error::external(format!("Invalid member ID hex: {}", e)))?;
    if bytes.len() != 32 {
        return Err(mlua::Error::external("Member ID must be 32 bytes"));
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    Ok(arr)
}

fn parse_realm_id(hex_str: &str) -> std::result::Result<RealmId, mlua::Error> {
    let bytes = hex::decode(hex_str)
        .map_err(|e| mlua::Error::external(format!("Invalid realm ID hex: {}", e)))?;
    if bytes.len() != 32 {
        return Err(mlua::Error::external("Realm ID must be 32 bytes"));
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    Ok(RealmId::from(arr))
}

fn parse_artifact_id(hex_str: &str) -> std::result::Result<ArtifactId, mlua::Error> {
    let bytes = hex::decode(hex_str)
        .map_err(|e| mlua::Error::external(format!("Invalid artifact ID hex: {}", e)))?;
    if bytes.len() != 32 {
        return Err(mlua::Error::external("Artifact ID must be 32 bytes"));
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    Ok(ArtifactId::Blob(arr))
}

fn artifact_id_to_hex(id: &ArtifactId) -> String {
    hex::encode(id.bytes())
}

fn parse_access_mode(s: &str) -> std::result::Result<AccessMode, mlua::Error> {
    match s {
        "revocable" => Ok(AccessMode::Revocable),
        "permanent" => Ok(AccessMode::Permanent),
        "transfer" => Ok(AccessMode::Transfer),
        _ => Err(mlua::Error::external(format!(
            "Unknown access mode: {}. Use 'revocable', 'permanent', or 'transfer'",
            s
        ))),
    }
}

/// Reconstruct a MessageId from a realm and sequence number.
fn make_message_id(realm_id: RealmId, sequence: u64) -> MessageId {
    use indras_core::EventId;
    MessageId::new(realm_id, EventId::new(0, sequence))
}

/// Convert a Message to a Lua table.
fn message_to_table(lua: &Lua, msg: &Message) -> Result<Table> {
    let t = lua.create_table()?;
    t.set("id", msg.id.event_id.sequence)?;
    t.set("sender_id", hex::encode(msg.sender.id()))?;
    t.set("sender_name", msg.sender.name())?;
    match &msg.content {
        Content::Text(s) => {
            t.set("type", "text")?;
            t.set("content", s.as_str())?;
        }
        Content::Reaction { target, emoji } => {
            t.set("type", "reaction")?;
            t.set("content", emoji.as_str())?;
            t.set("reaction_target", target.event_id.sequence)?;
        }
        Content::Artifact(reference) => {
            t.set("type", "artifact")?;
            t.set("content", format!("{:?}", reference))?;
        }
        _ => {
            t.set("type", "other")?;
            t.set("content", format!("{:?}", msg.content))?;
        }
    }
    t.set("timestamp", msg.timestamp.timestamp())?;
    Ok(t)
}

// ============================================================
// LuaNetwork — wraps IndrasNetwork
// ============================================================

/// Lua wrapper for IndrasNetwork.
///
/// Uses `Arc<tokio::sync::RwLock<IndrasNetwork>>` because `set_display_name`
/// requires `&mut self` while most other methods take `&self`.
struct LuaNetwork {
    network: Arc<tokio::sync::RwLock<IndrasNetwork>>,
    _temp_dir: Option<tempfile::TempDir>,
}

impl UserData for LuaNetwork {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // -- Lifecycle --

        methods.add_async_method("start", |_, this, ()| async move {
            let net = this.network.read().await;
            net.start().await.map_err(mlua::Error::external)
        });

        methods.add_async_method("stop", |_, this, ()| async move {
            let net = this.network.read().await;
            net.stop().await.map_err(mlua::Error::external)
        });

        methods.add_async_method("is_running", |_, this, ()| async move {
            let net = this.network.read().await;
            Ok(net.is_running())
        });

        // -- Identity --

        methods.add_async_method("id", |_, this, ()| async move {
            let net = this.network.read().await;
            Ok(hex::encode(net.id()))
        });

        methods.add_async_method("display_name", |_, this, ()| async move {
            let net = this.network.read().await;
            Ok(net.display_name().map(|s| s.to_string()))
        });

        methods.add_async_method("set_display_name", |_, this, name: String| async move {
            let mut net = this.network.write().await;
            net.set_display_name(name)
                .await
                .map_err(mlua::Error::external)
        });

        methods.add_async_method("identity_code", |_, this, ()| async move {
            let net = this.network.read().await;
            Ok(net.identity_code())
        });

        methods.add_async_method("identity_uri", |_, this, ()| async move {
            let net = this.network.read().await;
            Ok(net.identity_uri())
        });

        // -- Realm operations --

        methods.add_async_method("create_realm", |_, this, name: String| async move {
            let net = this.network.read().await;
            let realm = net.create_realm(&name).await.map_err(mlua::Error::external)?;
            Ok(LuaRealm { realm })
        });

        methods.add_async_method("join", |_, this, invite: String| async move {
            let net = this.network.read().await;
            let realm = net.join(&invite).await.map_err(mlua::Error::external)?;
            Ok(LuaRealm { realm })
        });

        methods.add_async_method("realms", |_, this, ()| async move {
            let net = this.network.read().await;
            let ids: Vec<String> = net
                .realms()
                .iter()
                .map(|id| hex::encode(id.as_bytes()))
                .collect();
            Ok(ids)
        });

        methods.add_async_method("get_realm", |_, this, realm_id_hex: String| async move {
            let rid = parse_realm_id(&realm_id_hex)?;
            let net = this.network.read().await;
            Ok(net.get_realm_by_id(&rid).map(|realm| LuaRealm { realm }))
        });

        methods.add_async_method("leave_realm", |_, this, realm_id_hex: String| async move {
            let rid = parse_realm_id(&realm_id_hex)?;
            let net = this.network.read().await;
            net.leave_realm(&rid).await.map_err(mlua::Error::external)
        });

        // -- Direct connection --

        methods.add_async_method("connect", |_, this, peer_id_hex: String| async move {
            let peer_id = parse_member_id(&peer_id_hex)?;
            let net = this.network.read().await;
            let realm = net.connect(peer_id).await.map_err(mlua::Error::external)?;
            Ok(LuaRealm { realm })
        });

        methods.add_async_method("connect_by_code", |_, this, code: String| async move {
            let net = this.network.read().await;
            let realm = net
                .connect_by_code(&code)
                .await
                .map_err(mlua::Error::external)?;
            Ok(LuaRealm { realm })
        });

        // -- Special realms --

        methods.add_async_method("home_realm", |_, this, ()| async move {
            let net = this.network.read().await;
            let home = net.home_realm().await.map_err(mlua::Error::external)?;
            Ok(LuaHomeRealm { home })
        });

        methods.add_async_method("contacts_realm", |_, this, ()| async move {
            let net = this.network.read().await;
            let contacts = net
                .join_contacts_realm()
                .await
                .map_err(mlua::Error::external)?;
            Ok(LuaContactsRealm { contacts })
        });

        // -- Identity export --

        methods.add_async_method("export_identity", |_, this, ()| async move {
            let net = this.network.read().await;
            let bytes = net
                .export_identity()
                .await
                .map_err(mlua::Error::external)?;
            Ok(STANDARD.encode(&bytes))
        });

        // -- Connect to another LuaNetwork by endpoint address (in-process) --

        methods.add_async_method(
            "connect_to",
            |_, this, other: mlua::AnyUserData| async move {
                let other_ref = other.borrow::<LuaNetwork>()?;
                let other_net = other_ref.network.read().await;
                let addr = other_net
                    .node()
                    .endpoint_addr()
                    .await
                    .ok_or_else(|| mlua::Error::external("Target network not started"))?;
                drop(other_net);
                drop(other_ref);

                let net = this.network.read().await;
                net.node()
                    .connect_by_addr(addr)
                    .await
                    .map_err(mlua::Error::external)
            },
        );

        // -- ToString --

        methods.add_async_method("__tostring_async", |_, this, ()| async move {
            let net = this.network.read().await;
            Ok(format!(
                "Network(id={}, running={})",
                hex::encode(&net.id()[..4]),
                net.is_running()
            ))
        });

        methods.add_meta_method(MetaMethod::ToString, |_, _this, ()| {
            Ok("Network(...)".to_string())
        });
    }
}

// ============================================================
// LuaRealm — wraps Realm
// ============================================================

/// Lua wrapper for Realm.
///
/// Realm is Clone, so we store it directly.
struct LuaRealm {
    realm: Realm,
}

impl UserData for LuaRealm {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // -- Properties --

        methods.add_method("id", |_, this, ()| {
            Ok(hex::encode(this.realm.id().as_bytes()))
        });

        methods.add_method("name", |_, this, ()| {
            Ok(this.realm.name().map(|s| s.to_string()))
        });

        methods.add_method("invite_code", |_, this, ()| {
            Ok(this.realm.invite_code().map(|ic| ic.to_string()))
        });

        // -- Messaging --

        methods.add_async_method("send", |_, this, content: String| async move {
            let msg_id = this
                .realm
                .send(content)
                .await
                .map_err(mlua::Error::external)?;
            Ok(msg_id.event_id.sequence)
        });

        methods.add_async_method(
            "reply",
            |_, this, (sequence, content): (u64, String)| async move {
                let msg_id = make_message_id(this.realm.id(), sequence);
                let reply_id = this
                    .realm
                    .reply(msg_id, content)
                    .await
                    .map_err(mlua::Error::external)?;
                Ok(reply_id.event_id.sequence)
            },
        );

        methods.add_async_method(
            "react",
            |_, this, (sequence, emoji): (u64, String)| async move {
                let msg_id = make_message_id(this.realm.id(), sequence);
                let react_id = this
                    .realm
                    .react(msg_id, emoji)
                    .await
                    .map_err(mlua::Error::external)?;
                Ok(react_id.event_id.sequence)
            },
        );

        methods.add_async_method("messages_since", |lua, this, since_seq: u64| async move {
            let messages = this
                .realm
                .messages_since(since_seq)
                .await
                .map_err(mlua::Error::external)?;
            let result = lua.create_table()?;
            for (i, msg) in messages.iter().enumerate() {
                let t = message_to_table(&lua, msg)?;
                result.set(i + 1, t)?;
            }
            Ok(result)
        });

        methods.add_async_method("all_messages", |lua, this, ()| async move {
            let messages = this
                .realm
                .all_messages()
                .await
                .map_err(mlua::Error::external)?;
            let result = lua.create_table()?;
            for (i, msg) in messages.iter().enumerate() {
                let t = message_to_table(&lua, msg)?;
                result.set(i + 1, t)?;
            }
            Ok(result)
        });

        methods.add_async_method("search_messages", |lua, this, query: String| async move {
            let messages = this
                .realm
                .search_messages(&query)
                .await
                .map_err(mlua::Error::external)?;
            let result = lua.create_table()?;
            for (i, msg) in messages.iter().enumerate() {
                let t = message_to_table(&lua, msg)?;
                result.set(i + 1, t)?;
            }
            Ok(result)
        });

        // -- Members --

        methods.add_async_method("member_list", |lua, this, ()| async move {
            let members = this
                .realm
                .member_list()
                .await
                .map_err(mlua::Error::external)?;
            let result = lua.create_table()?;
            for (i, member) in members.iter().enumerate() {
                let entry = lua.create_table()?;
                entry.set("id", hex::encode(member.id()))?;
                entry.set("name", member.name())?;
                result.set(i + 1, entry)?;
            }
            Ok(result)
        });

        methods.add_async_method("member_count", |_, this, ()| async move {
            let count = this
                .realm
                .member_count()
                .await
                .map_err(mlua::Error::external)?;
            Ok(count)
        });

        // -- Read tracking --

        methods.add_async_method("mark_read", |_, this, member_id_hex: String| async move {
            let member_id = parse_member_id(&member_id_hex)?;
            this.realm
                .mark_read(member_id)
                .await
                .map_err(mlua::Error::external)
        });

        methods.add_async_method(
            "unread_count",
            |_, this, member_id_hex: String| async move {
                let member_id = parse_member_id(&member_id_hex)?;
                let count = this
                    .realm
                    .unread_count(&member_id)
                    .await
                    .map_err(mlua::Error::external)?;
                Ok(count)
            },
        );

        methods.add_async_method(
            "last_read_seq",
            |_, this, member_id_hex: String| async move {
                let member_id = parse_member_id(&member_id_hex)?;
                let seq = this
                    .realm
                    .last_read_seq(&member_id)
                    .await
                    .map_err(mlua::Error::external)?;
                Ok(seq)
            },
        );

        // -- Documents --

        methods.add_async_method("document", |_, this, name: String| async move {
            let doc = this
                .realm
                .document::<LuaJsonDoc>(&name)
                .await
                .map_err(mlua::Error::external)?;
            Ok(LuaDocument { doc })
        });

        methods.add_async_method("document_names", |_, this, ()| async move {
            let names = this
                .realm
                .document_names()
                .await
                .map_err(mlua::Error::external)?;
            Ok(names)
        });

        // -- CRDT Chat (Document<RealmChatDocument>) --

        methods.add_async_method("chat_send", |_, this, (author, text): (String, String)| async move {
            let id = this.realm
                .chat_send(&author, text)
                .await
                .map_err(mlua::Error::external)?;
            Ok(id)
        });

        methods.add_async_method(
            "chat_reply",
            |_, this, (author, parent_id, text): (String, String, String)| async move {
                let id = this.realm
                    .chat_reply(&author, &parent_id, text)
                    .await
                    .map_err(mlua::Error::external)?;
                Ok(id)
            },
        );

        methods.add_async_method(
            "chat_react",
            |_, this, (author, msg_id, emoji): (String, String, String)| async move {
                let result = this.realm
                    .chat_react(&author, &msg_id, &emoji)
                    .await
                    .map_err(mlua::Error::external)?;
                Ok(result)
            },
        );

        methods.add_async_method("chat_doc", |_, this, ()| async move {
            let doc = this.realm.chat_doc().await.map_err(mlua::Error::external)?;
            Ok(LuaChatDoc { doc: doc.clone() })
        });

        // -- Alias --

        methods.add_async_method("set_alias", |_, this, alias: String| async move {
            this.realm
                .set_alias(alias)
                .await
                .map_err(mlua::Error::external)
        });

        methods.add_async_method("get_alias", |_, this, ()| async move {
            this.realm
                .get_alias()
                .await
                .map_err(mlua::Error::external)
        });

        methods.add_async_method("clear_alias", |_, this, ()| async move {
            this.realm
                .clear_alias()
                .await
                .map_err(mlua::Error::external)
        });

        // -- ToString --

        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            let id_hex = hex::encode(&this.realm.id().as_bytes()[..4]);
            let name = this
                .realm
                .name()
                .map(|s| s.to_string())
                .unwrap_or_else(|| "unnamed".to_string());
            Ok(format!("Realm(id={}, name={})", id_hex, name))
        });
    }
}

// ============================================================
// LuaDocument — wraps Document<LuaJsonDoc>
// ============================================================

/// Lua wrapper for Document<LuaJsonDoc>.
struct LuaDocument {
    doc: Document<LuaJsonDoc>,
}

impl UserData for LuaDocument {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("name", |_, this, ()| Ok(this.doc.name().to_string()));

        methods.add_async_method("read", |lua, this, ()| async move {
            // Auto-refresh to pick up remote changes synced via CRDT
            let _ = this.doc.refresh().await;
            let guard = this.doc.read().await;
            let val = &guard.0;
            lua.to_value(val)
        });

        methods.add_async_method("refresh", |_, this, ()| async move {
            this.doc
                .refresh()
                .await
                .map_err(mlua::Error::external)
        });

        methods.add_async_method("update", |lua, this, lua_val: Value| async move {
            let val: serde_json::Value = lua.from_value(lua_val)?;
            this.doc
                .update(|d| d.0 = val)
                .await
                .map_err(mlua::Error::external)
        });

        methods.add_async_method("merge", |lua, this, lua_val: Value| async move {
            let remote: serde_json::Value = lua.from_value(lua_val)?;
            this.doc
                .update(|d| {
                    match (&mut d.0, remote) {
                        (serde_json::Value::Object(a), serde_json::Value::Object(b)) => {
                            for (k, v) in b {
                                a.insert(k, v);
                            }
                        }
                        _ => {
                            // If either side isn't an object, skip
                        }
                    }
                })
                .await
                .map_err(mlua::Error::external)
        });

        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            Ok(format!("Document(name={})", this.doc.name()))
        });
    }
}

// ============================================================
// LuaChatDoc — wraps Document<RealmChatDocument>
// ============================================================

/// Lua wrapper for Document<RealmChatDocument> providing CRDT chat access.
struct LuaChatDoc {
    doc: Document<RealmChatDocument>,
}

impl UserData for LuaChatDoc {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_async_method("visible_messages", |lua, this, ()| async move {
            // Refresh to pick up remote changes
            let _ = this.doc.refresh().await;
            let guard = this.doc.read().await;
            let msgs = guard.visible_messages();
            let result = lua.create_table()?;
            for (i, msg) in msgs.iter().enumerate() {
                let t = lua.create_table()?;
                t.set("id", msg.id.as_str())?;
                t.set("author", msg.author.as_str())?;
                if let Some(ref aid) = msg.author_id {
                    t.set("author_id", aid.as_str())?;
                }
                t.set("content", msg.current_content.as_str())?;
                t.set("created_at", msg.created_at)?;
                t.set("is_deleted", msg.is_deleted)?;
                t.set("type", format!("{:?}", msg.message_type))?;
                if let Some(ref reply_to) = msg.reply_to {
                    t.set("reply_to", reply_to.as_str())?;
                }
                // Convert reactions HashMap to Lua table
                if !msg.reactions.is_empty() {
                    let reactions_table = lua.create_table()?;
                    for (emoji, authors) in &msg.reactions {
                        let authors_table = lua.create_table()?;
                        for (j, author) in authors.iter().enumerate() {
                            authors_table.set(j + 1, author.as_str())?;
                        }
                        reactions_table.set(emoji.as_str(), authors_table)?;
                    }
                    t.set("reactions", reactions_table)?;
                }
                result.set(i + 1, t)?;
            }
            Ok(result)
        });

        methods.add_async_method("visible_count", |_, this, ()| async move {
            let _ = this.doc.refresh().await;
            let guard = this.doc.read().await;
            Ok(guard.visible_count())
        });

        methods.add_async_method("refresh", |_, this, ()| async move {
            this.doc.refresh().await.map_err(mlua::Error::external)
        });

        methods.add_async_method("poll_change", |_, this, timeout_secs: f64| async move {
            use indras_network::prelude::StreamExt;
            let mut stream = this.doc.changes();
            let got_item = tokio::time::timeout(
                std::time::Duration::from_secs_f64(timeout_secs),
                stream.next(),
            )
            .await
            .is_ok();
            Ok(got_item)
        });

        methods.add_method("subscribe", |_, this, ()| {
            let rx = this.doc.subscribe();
            Ok(LuaChatSubscription {
                rx: tokio::sync::Mutex::new(rx),
            })
        });

        methods.add_meta_method(MetaMethod::ToString, |_, _this, ()| {
            Ok("ChatDoc(chat)".to_string())
        });
    }
}

// ============================================================
// LuaChatSubscription — pre-created broadcast receiver
// ============================================================

/// Lua wrapper for a pre-created change subscription.
///
/// Created via `doc:subscribe()` BEFORE messages are sent, then
/// `sub:wait(timeout)` to deterministically test push notification.
struct LuaChatSubscription {
    rx: tokio::sync::Mutex<tokio::sync::broadcast::Receiver<
        indras_network::document::DocumentChange<RealmChatDocument>,
    >>,
}

impl UserData for LuaChatSubscription {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_async_method("wait", |_, this, timeout_secs: f64| async move {
            let mut rx = this.rx.lock().await;
            let got = tokio::time::timeout(
                std::time::Duration::from_secs_f64(timeout_secs),
                rx.recv(),
            )
            .await
            .is_ok();
            Ok(got)
        });

        methods.add_meta_method(MetaMethod::ToString, |_, _this, ()| {
            Ok("ChatSubscription".to_string())
        });
    }
}

// ============================================================
// LuaHomeRealm — wraps HomeRealm
// ============================================================

/// Lua wrapper for HomeRealm.
struct LuaHomeRealm {
    home: HomeRealm,
}

impl UserData for LuaHomeRealm {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("id", |_, this, ()| {
            Ok(hex::encode(this.home.id().as_bytes()))
        });

        // -- Artifacts --

        methods.add_async_method("upload", |_, this, path: String| async move {
            let id = this
                .home
                .upload(&path)
                .await
                .map_err(mlua::Error::external)?;
            Ok(artifact_id_to_hex(&id))
        });

        methods.add_async_method(
            "get_artifact",
            |lua, this, artifact_id_hex: String| async move {
                let id = parse_artifact_id(&artifact_id_hex)?;
                let data = this
                    .home
                    .get_artifact(&id)
                    .await
                    .map_err(mlua::Error::external)?;
                // Return as a Lua string (binary-safe in Lua 5.4)
                Ok(lua.create_string(&data)?)
            },
        );

        methods.add_async_method(
            "grant_access",
            |_, this, (artifact_id_hex, grantee_hex, mode_str): (String, String, String)| async move {
                let id = parse_artifact_id(&artifact_id_hex)?;
                let grantee = parse_member_id(&grantee_hex)?;
                let mode = parse_access_mode(&mode_str)?;
                this.home
                    .grant_access(&id, grantee, mode)
                    .await
                    .map_err(mlua::Error::external)
            },
        );

        methods.add_async_method(
            "revoke_access",
            |_, this, (artifact_id_hex, grantee_hex): (String, String)| async move {
                let id = parse_artifact_id(&artifact_id_hex)?;
                let grantee = parse_member_id(&grantee_hex)?;
                this.home
                    .revoke_access(&id, &grantee)
                    .await
                    .map_err(mlua::Error::external)
            },
        );

        methods.add_async_method(
            "recall",
            |_, this, artifact_id_hex: String| async move {
                let id = parse_artifact_id(&artifact_id_hex)?;
                this.home
                    .recall(&id)
                    .await
                    .map_err(mlua::Error::external)
            },
        );

        methods.add_async_method(
            "shared_with",
            |lua, this, member_id_hex: String| async move {
                let member_id = parse_member_id(&member_id_hex)?;
                let entries = this
                    .home
                    .shared_with(&member_id)
                    .await
                    .map_err(mlua::Error::external)?;
                let result = lua.create_table()?;
                for (i, entry) in entries.iter().enumerate() {
                    let t = lua.create_table()?;
                    t.set("id", artifact_id_to_hex(&entry.id))?;
                    t.set("name", entry.name.as_str())?;
                    t.set("size", entry.size)?;
                    t.set(
                        "mime_type",
                        entry.mime_type.as_ref().map(|s| s.as_str()),
                    )?;
                    result.set(i + 1, t)?;
                }
                Ok(result)
            },
        );

        // -- Documents --

        methods.add_async_method("document", |_, this, name: String| async move {
            let doc = this
                .home
                .document::<LuaJsonDoc>(&name)
                .await
                .map_err(mlua::Error::external)?;
            Ok(LuaDocument { doc })
        });

        // -- ToString --

        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            Ok(format!(
                "HomeRealm(id={})",
                hex::encode(&this.home.id().as_bytes()[..4])
            ))
        });
    }
}

// ============================================================
// LuaContactsRealm — wraps ContactsRealm
// ============================================================

/// Lua wrapper for ContactsRealm.
struct LuaContactsRealm {
    contacts: ContactsRealm,
}

impl UserData for LuaContactsRealm {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("id", |_, this, ()| {
            Ok(hex::encode(this.contacts.id().as_bytes()))
        });

        methods.add_async_method(
            "add_contact",
            |_, this, member_id_hex: String| async move {
                let member_id = parse_member_id(&member_id_hex)?;
                this.contacts
                    .add_contact(member_id)
                    .await
                    .map_err(mlua::Error::external)
            },
        );

        methods.add_async_method(
            "add_contact_with_name",
            |_, this, (member_id_hex, name): (String, String)| async move {
                let member_id = parse_member_id(&member_id_hex)?;
                this.contacts
                    .add_contact_with_name(member_id, Some(name))
                    .await
                    .map_err(mlua::Error::external)
            },
        );

        methods.add_async_method(
            "remove_contact",
            |_, this, member_id_hex: String| async move {
                let member_id = parse_member_id(&member_id_hex)?;
                this.contacts
                    .remove_contact(&member_id)
                    .await
                    .map_err(mlua::Error::external)
            },
        );

        methods.add_async_method(
            "is_contact",
            |_, this, member_id_hex: String| async move {
                let member_id = parse_member_id(&member_id_hex)?;
                Ok(this.contacts.is_contact(&member_id).await)
            },
        );

        methods.add_async_method("contacts_list", |_, this, ()| async move {
            let ids: Vec<String> = this
                .contacts
                .contacts_list_async()
                .await
                .iter()
                .map(hex::encode)
                .collect();
            Ok(ids)
        });

        methods.add_async_method("contact_count", |_, this, ()| async move {
            Ok(this.contacts.contact_count_async().await)
        });

        methods.add_async_method(
            "confirm_contact",
            |_, this, member_id_hex: String| async move {
                let member_id = parse_member_id(&member_id_hex)?;
                this.contacts
                    .confirm_contact(&member_id)
                    .await
                    .map_err(mlua::Error::external)
            },
        );

        methods.add_async_method("get_status", |_, this, member_id_hex: String| async move {
            let member_id = parse_member_id(&member_id_hex)?;
            Ok(this.contacts.get_status_async(&member_id).await.map(|s| match s {
                indras_network::ContactStatus::Pending => "pending".to_string(),
                indras_network::ContactStatus::Confirmed => "confirmed".to_string(),
            }))
        });

        methods.add_async_method(
            "update_sentiment",
            |_, this, (member_id_hex, sentiment): (String, i8)| async move {
                let member_id = parse_member_id(&member_id_hex)?;
                this.contacts
                    .update_sentiment(&member_id, sentiment)
                    .await
                    .map_err(mlua::Error::external)
            },
        );

        methods.add_async_method("get_sentiment", |_, this, member_id_hex: String| async move {
            let member_id = parse_member_id(&member_id_hex)?;
            Ok(this.contacts.get_sentiment_async(&member_id).await)
        });

        methods.add_async_method(
            "set_relayable",
            |_, this, (member_id_hex, relayable): (String, bool)| async move {
                let member_id = parse_member_id(&member_id_hex)?;
                this.contacts
                    .set_relayable(&member_id, relayable)
                    .await
                    .map_err(mlua::Error::external)
            },
        );

        // -- ToString --

        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            Ok(format!(
                "ContactsRealm(id={})",
                hex::encode(&this.contacts.id().as_bytes()[..4])
            ))
        });
    }
}

// ============================================================
// Register function
// ============================================================

/// Register Network bindings with the indras Lua table.
pub fn register(lua: &Lua, indras: &Table) -> Result<()> {
    let network_table = lua.create_table()?;

    // Network.new(path?) -> LuaNetwork
    network_table.set(
        "new",
        lua.create_async_function(|_, path: Option<String>| async move {
            let (data_path, temp_dir) = match path {
                Some(p) => (std::path::PathBuf::from(p), None),
                None => {
                    let tmp = tempfile::TempDir::new().map_err(mlua::Error::external)?;
                    let path = tmp.path().to_path_buf();
                    (path, Some(tmp))
                }
            };

            let network = IndrasNetwork::new(&data_path)
                .await
                .map_err(mlua::Error::external)?;

            Ok(LuaNetwork {
                network: Arc::new(tokio::sync::RwLock::new(network)),
                _temp_dir: temp_dir,
            })
        })?,
    )?;

    indras.set("Network", network_table)?;

    Ok(())
}
