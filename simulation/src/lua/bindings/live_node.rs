//! Lua bindings for real IndrasNode (live P2P networking)
//!
//! Unlike the simulation bindings which model P2P behavior in-memory,
//! these bindings wrap real `IndrasNode` instances with actual QUIC transport,
//! CRDT sync, and post-quantum cryptography.

use mlua::{Lua, MetaMethod, Result, Table, UserData, UserDataMethods};
use std::path::PathBuf;
use std::sync::Arc;

use indras_node::{IndrasNode, InviteKey, NodeConfig};
use indras_core::{InterfaceId, PeerIdentity, transport::Transport};

/// Lua wrapper for a real IndrasNode
///
/// Holds the node in an Arc (shared ownership for async methods)
/// and optionally a TempDir to keep temporary storage alive.
struct LuaLiveNode {
    node: Arc<IndrasNode>,
    /// Data directory path (for recreating nodes with same state)
    data_dir: PathBuf,
    /// Kept alive to prevent temp dir cleanup
    _temp_dir: Option<tempfile::TempDir>,
}

impl UserData for LuaLiveNode {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // -- Lifecycle --

        methods.add_async_method("start", |_, this, ()| async move {
            this.node
                .start()
                .await
                .map_err(mlua::Error::external)
        });

        methods.add_async_method("stop", |_, this, ()| async move {
            this.node
                .stop()
                .await
                .map_err(mlua::Error::external)
        });

        methods.add_method("is_started", |_, this, ()| {
            Ok(this.node.is_started())
        });

        // -- Identity --

        methods.add_method("identity", |_, this, ()| {
            Ok(this.node.identity().short_id())
        });

        methods.add_method("identity_full", |_, this, ()| {
            Ok(hex::encode(this.node.identity().as_bytes()))
        });

        // -- Interfaces --

        methods.add_async_method(
            "create_interface",
            |_, this, name: Option<String>| async move {
                let (iface_id, invite) = this
                    .node
                    .create_interface(name.as_deref())
                    .await
                    .map_err(mlua::Error::external)?;

                let iface_hex = hex::encode(iface_id.as_bytes());
                let invite_b64 = invite
                    .to_base64()
                    .map_err(mlua::Error::external)?;

                Ok((iface_hex, invite_b64))
            },
        );

        methods.add_async_method(
            "join_interface",
            |_, this, invite_b64: String| async move {
                let invite = InviteKey::from_base64(&invite_b64)
                    .map_err(mlua::Error::external)?;
                let iface_id = this
                    .node
                    .join_interface(invite)
                    .await
                    .map_err(mlua::Error::external)?;
                Ok(hex::encode(iface_id.as_bytes()))
            },
        );

        // -- Messaging --

        methods.add_async_method(
            "send_message",
            |_, this, (iface_hex, content): (String, String)| async move {
                let iface_bytes = hex::decode(&iface_hex)
                    .map_err(|e| mlua::Error::external(format!("Invalid interface ID hex: {}", e)))?;
                if iface_bytes.len() != 32 {
                    return Err(mlua::Error::external("Interface ID must be 32 bytes"));
                }
                let mut bytes = [0u8; 32];
                bytes.copy_from_slice(&iface_bytes);
                let iface_id = InterfaceId::from(bytes);

                let event_id = this
                    .node
                    .send_message(&iface_id, content.into_bytes())
                    .await
                    .map_err(mlua::Error::external)?;

                Ok(event_id.sequence)
            },
        );

        // -- Reading events --

        methods.add_async_method(
            "events_since",
            |lua, this, (iface_hex, since): (String, u64)| async move {
                let iface_bytes = hex::decode(&iface_hex)
                    .map_err(|e| mlua::Error::external(format!("Invalid interface ID hex: {}", e)))?;
                if iface_bytes.len() != 32 {
                    return Err(mlua::Error::external("Interface ID must be 32 bytes"));
                }
                let mut bytes = [0u8; 32];
                bytes.copy_from_slice(&iface_bytes);
                let iface_id = InterfaceId::from(bytes);

                let events = this
                    .node
                    .events_since(&iface_id, since)
                    .await
                    .map_err(mlua::Error::external)?;

                let result = lua.create_table()?;
                for (i, event) in events.iter().enumerate() {
                    let entry = lua.create_table()?;
                    match event {
                        indras_core::InterfaceEvent::Message {
                            sender,
                            content,
                            id,
                            ..
                        } => {
                            entry.set("sender", sender.short_id())?;
                            entry.set(
                                "content",
                                String::from_utf8_lossy(content).to_string(),
                            )?;
                            entry.set("sequence", id.sequence)?;
                        }
                        _ => {
                            entry.set("sender", "system")?;
                            entry.set("content", format!("{:?}", event))?;
                            entry.set("sequence", 0u64)?;
                        }
                    }
                    result.set(i + 1, entry)?;
                }
                Ok(result)
            },
        );

        // -- Reading document events (includes CRDT-synced remote events) --

        methods.add_async_method(
            "document_events",
            |lua, this, iface_hex: String| async move {
                let iface_bytes = hex::decode(&iface_hex)
                    .map_err(|e| mlua::Error::external(format!("Invalid interface ID hex: {}", e)))?;
                if iface_bytes.len() != 32 {
                    return Err(mlua::Error::external("Interface ID must be 32 bytes"));
                }
                let mut bytes = [0u8; 32];
                bytes.copy_from_slice(&iface_bytes);
                let iface_id = InterfaceId::from(bytes);

                let events = this
                    .node
                    .document_events(&iface_id)
                    .await
                    .map_err(mlua::Error::external)?;

                let result = lua.create_table()?;
                for (i, event) in events.iter().enumerate() {
                    let entry = lua.create_table()?;
                    match event {
                        indras_core::InterfaceEvent::Message {
                            sender,
                            content,
                            id,
                            ..
                        } => {
                            entry.set("sender", sender.short_id())?;
                            entry.set(
                                "content",
                                String::from_utf8_lossy(content).to_string(),
                            )?;
                            entry.set("sequence", id.sequence)?;
                        }
                        _ => {
                            entry.set("sender", "system")?;
                            entry.set("content", format!("{:?}", event))?;
                            entry.set("sequence", 0u64)?;
                        }
                    }
                    result.set(i + 1, entry)?;
                }
                Ok(result)
            },
        );

        // -- Members --

        methods.add_async_method("members", |_, this, iface_hex: String| async move {
            let iface_bytes = hex::decode(&iface_hex)
                .map_err(|e| mlua::Error::external(format!("Invalid interface ID hex: {}", e)))?;
            if iface_bytes.len() != 32 {
                return Err(mlua::Error::external("Interface ID must be 32 bytes"));
            }
            let mut bytes = [0u8; 32];
            bytes.copy_from_slice(&iface_bytes);
            let iface_id = InterfaceId::from(bytes);

            let members = this
                .node
                .members(&iface_id)
                .await
                .map_err(mlua::Error::external)?;

            let result: Vec<String> = members.iter().map(|m| m.short_id()).collect();
            Ok(result)
        });

        // -- Connect to peer --

        methods.add_async_method(
            "connect_to_peer",
            |_, this, peer_hex: String| async move {
                let peer_bytes = hex::decode(&peer_hex)
                    .map_err(|e| mlua::Error::external(format!("Invalid peer hex: {}", e)))?;
                if peer_bytes.len() != 32 {
                    return Err(mlua::Error::external("Peer key must be 32 bytes"));
                }
                let mut key = [0u8; 32];
                key.copy_from_slice(&peer_bytes);
                this.node
                    .connect_to_peer(&key)
                    .await
                    .map_err(mlua::Error::external)
            },
        );

        // -- Disconnect from peer (close stale connection) --

        methods.add_async_method(
            "disconnect_from",
            |_, this, peer_hex: String| async move {
                let peer_bytes = hex::decode(&peer_hex)
                    .map_err(|e| mlua::Error::external(format!("Invalid peer hex: {}", e)))?;
                if peer_bytes.len() != 32 {
                    return Err(mlua::Error::external("Peer key must be 32 bytes"));
                }
                let mut key = [0u8; 32];
                key.copy_from_slice(&peer_bytes);
                this.node
                    .disconnect_from(&key)
                    .await
                    .map_err(mlua::Error::external)
            },
        );

        // -- Connect to another node by its endpoint address (in-process) --
        // Unlike connect_to_peer (relay discovery), this uses full address info

        methods.add_async_method(
            "connect_to",
            |_, this, other: mlua::AnyUserData| async move {
                let other_ref = other.borrow::<LuaLiveNode>()?;
                let addr = other_ref
                    .node
                    .endpoint_addr()
                    .await
                    .ok_or_else(|| mlua::Error::external("Target node not started"))?;
                drop(other_ref);

                this.node
                    .connect_by_addr(addr)
                    .await
                    .map_err(mlua::Error::external)
            },
        );

        // -- World View (debug state snapshot) --

        methods.add_async_method("world_view", |lua, this, _iface_hex: Option<String>| async move {
            let result = lua.create_table()?;

            // Node identity
            result.set("identity", this.node.identity().short_id())?;
            result.set("identity_full", hex::encode(this.node.identity().as_bytes()))?;
            result.set("started", this.node.is_started())?;
            result.set("data_dir", this.data_dir.to_string_lossy().to_string())?;

            // Interfaces
            let iface_ids = this.node.list_interfaces();
            let ifaces_table = lua.create_table()?;
            for (i, iface_id) in iface_ids.iter().enumerate() {
                let iface_table = lua.create_table()?;
                let id_hex = hex::encode(iface_id.as_bytes());
                iface_table.set("id", id_hex.clone())?;

                // In-memory members (what sync_task iterates)
                let members = this.node.members(iface_id).await
                    .unwrap_or_default();
                let members_table = lua.create_table()?;
                for (j, m) in members.iter().enumerate() {
                    members_table.set(j + 1, m.short_id())?;
                }
                iface_table.set("members", members_table)?;
                iface_table.set("member_count", members.len())?;

                // Storage members (what survives restart)
                let storage = this.node.storage();
                let storage_members = storage.interface_store()
                    .get_members(iface_id)
                    .unwrap_or_default();
                let storage_table = lua.create_table()?;
                for (j, m) in storage_members.iter().enumerate() {
                    let peer_hex = m.peer_id.iter().map(|b| format!("{:02x}", b)).collect::<String>();
                    // Show short id (first 10 hex chars)
                    let short = if peer_hex.len() >= 10 { &peer_hex[..10] } else { &peer_hex };
                    storage_table.set(j + 1, short.to_string())?;
                }
                iface_table.set("storage_members", storage_table)?;
                iface_table.set("storage_member_count", storage_members.len())?;

                ifaces_table.set(i + 1, iface_table)?;
            }
            result.set("interfaces", ifaces_table)?;
            result.set("interface_count", iface_ids.len())?;

            // Connected peers (transport layer)
            if let Some(transport) = this.node.transport().await {
                let connected: Vec<String> = transport.connected_peers()
                    .into_iter()
                    .map(|p| p.short_id())
                    .collect();
                let connected_table = lua.create_table()?;
                for (i, p) in connected.iter().enumerate() {
                    connected_table.set(i + 1, p.clone())?;
                }
                result.set("connected_peers", connected_table)?;
                result.set("connected_count", connected.len())?;
            } else {
                result.set("connected_peers", lua.create_table()?)?;
                result.set("connected_count", 0)?;
            }

            Ok(result)
        });

        // -- Data dir (for recreating node with same state) --

        methods.add_method("data_dir", |_, this, ()| {
            Ok(this.data_dir.to_string_lossy().to_string())
        });

        // -- ToString --

        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            Ok(format!(
                "LiveNode(id={}, started={})",
                this.node.identity().short_id(),
                this.node.is_started()
            ))
        });
    }
}

/// Register LiveNode bindings with the indras Lua table
pub fn register(lua: &Lua, indras: &Table) -> Result<()> {
    let live_node = lua.create_table()?;

    // LiveNode.new(path?) -> LiveNode
    // If no path given, creates a temp directory that's auto-cleaned on drop
    live_node.set(
        "new",
        lua.create_async_function(|_, path: Option<String>| async move {
            let (data_path, temp_dir) = match path {
                Some(p) => {
                    let path = std::path::PathBuf::from(p);
                    (path, None)
                }
                None => {
                    let tmp = tempfile::TempDir::new()
                        .map_err(mlua::Error::external)?;
                    let path = tmp.path().to_path_buf();
                    (path, Some(tmp))
                }
            };

            let config = NodeConfig::with_data_dir(&data_path);
            let node = IndrasNode::new(config)
                .await
                .map_err(mlua::Error::external)?;

            Ok(LuaLiveNode {
                node: Arc::new(node),
                data_dir: data_path,
                _temp_dir: temp_dir,
            })
        })?,
    )?;

    indras.set("LiveNode", live_node)?;

    Ok(())
}
