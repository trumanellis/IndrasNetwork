//! Lua bindings for relay node and relay client (live relay networking)
//!
//! Wraps `RelayNode` and `RelayClient`/`RelaySession` for use in Lua scenarios.
//! Follows the same UserData pattern as `live_node.rs`.
//!
//! ## Usage
//!
//! ```lua
//! local relay = indras.RelayNode.new({ owner = true })
//! local owner = indras.RelayClient.new_as_owner(relay)
//! local ack = owner:authenticate()
//! owner:store_event("Self_", iface_hex, "hello")
//! relay:shutdown()
//! ```

use std::path::PathBuf;
use std::sync::Arc;

use ed25519_dalek::SigningKey;
use iroh::SecretKey;
use mlua::{Lua, MetaMethod, Result, Table, UserData, UserDataMethods};
use tokio::sync::Mutex;

use indras_relay::{RelayConfig, RelayNode};
use indras_transport::relay_client::{RelayClient, RelaySession};
use indras_transport::protocol::StorageTier;
use indras_core::InterfaceId;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn parse_tier(s: &str) -> mlua::Result<StorageTier> {
    match s {
        "Self_" => Ok(StorageTier::Self_),
        "Connections" => Ok(StorageTier::Connections),
        "Public" => Ok(StorageTier::Public),
        other => Err(mlua::Error::external(format!(
            "Unknown storage tier '{}'. Expected Self_, Connections, or Public",
            other
        ))),
    }
}

fn tier_to_string(t: StorageTier) -> &'static str {
    match t {
        StorageTier::Self_ => "Self_",
        StorageTier::Connections => "Connections",
        StorageTier::Public => "Public",
    }
}

fn hex_to_interface_id(hex_str: &str) -> mlua::Result<InterfaceId> {
    let bytes = hex::decode(hex_str)
        .map_err(|e| mlua::Error::external(format!("Invalid interface ID hex: {}", e)))?;
    if bytes.len() != 32 {
        return Err(mlua::Error::external(format!(
            "Interface ID must be 32 bytes, got {}",
            bytes.len()
        )));
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    Ok(InterfaceId::from(arr))
}

fn random_signing_key() -> SigningKey {
    let mut bytes = [0u8; 32];
    rand::fill(&mut bytes);
    SigningKey::from_bytes(&bytes)
}

// ---------------------------------------------------------------------------
// LuaRelayNode
// ---------------------------------------------------------------------------

/// Lua wrapper for a RelayNode
///
/// The relay is started during `new()` (after construction, `start()` is
/// called internally). Shutdown is triggered via `shutdown()`.
struct LuaRelayNode {
    /// Callable that cancels the relay's shutdown token
    shutdown_fn: Arc<dyn Fn() + Send + Sync>,
    /// Endpoint address — available after start
    endpoint_addr: Arc<iroh::EndpointAddr>,
    /// Data directory
    data_dir: PathBuf,
    /// Owner signing key — stored when owner=true so clients can reuse it
    owner_signing_key: Option<SigningKey>,
    /// Keep temp directory alive for the process lifetime
    _temp_dir: Option<tempfile::TempDir>,
    /// Background task handle — kept alive so the relay keeps running
    _handle: Option<tokio::task::JoinHandle<indras_relay::RelayResult<()>>>,
}

impl UserData for LuaRelayNode {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // -- shutdown() --
        methods.add_method("shutdown", |_, this, ()| {
            (this.shutdown_fn)();
            Ok(())
        });

        // -- data_dir() --
        methods.add_method("data_dir", |_, this, ()| {
            Ok(this.data_dir.to_string_lossy().to_string())
        });

        // -- owner_player_id() --
        methods.add_method("owner_player_id", |_, this, ()| {
            let hex = this.owner_signing_key.as_ref().map(|sk| {
                hex::encode(sk.verifying_key().to_bytes())
            });
            Ok(hex)
        });

        // -- ToString --
        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            Ok(format!(
                "RelayNode(data_dir={})",
                this.data_dir.display(),
            ))
        });
    }
}

// ---------------------------------------------------------------------------
// LuaRelayClient
// ---------------------------------------------------------------------------

/// Lua wrapper for a RelaySession
///
/// The session is wrapped in `Arc<Mutex<Option<...>>>` so async Lua methods
/// can access it while satisfying mlua's `UserData` ownership model.
struct LuaRelayClient {
    session: Arc<Mutex<Option<RelaySession>>>,
    signing_key: SigningKey,
    player_id: [u8; 32],
}

impl UserData for LuaRelayClient {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // -- authenticate() -> { authenticated, granted_tiers } --
        methods.add_async_method("authenticate", |lua, this, ()| async move {
            let mut guard = this.session.lock().await;
            let session = guard.as_mut().ok_or_else(|| {
                mlua::Error::external("Session closed")
            })?;

            let ack = session.authenticate().await.map_err(mlua::Error::external)?;

            let result = lua.create_table()?;
            result.set("authenticated", ack.authenticated)?;

            let tiers_table = lua.create_table()?;
            for (i, tier) in ack.granted_tiers.iter().enumerate() {
                tiers_table.set(i + 1, tier_to_string(*tier))?;
            }
            result.set("granted_tiers", tiers_table)?;

            Ok(result)
        });

        // -- register(interfaces_table) -> { accepted, rejected } --
        methods.add_async_method(
            "register",
            |lua, this, interfaces_table: Table| async move {
                let mut iface_ids: Vec<InterfaceId> = Vec::new();
                for pair in interfaces_table.pairs::<mlua::Value, String>() {
                    let (_, hex_str) = pair?;
                    iface_ids.push(hex_to_interface_id(&hex_str)?);
                }

                let mut guard = this.session.lock().await;
                let session = guard.as_mut().ok_or_else(|| {
                    mlua::Error::external("Session closed")
                })?;

                let ack = session.register(iface_ids).await.map_err(mlua::Error::external)?;

                let result = lua.create_table()?;

                let accepted_table = lua.create_table()?;
                for (i, iface) in ack.accepted.iter().enumerate() {
                    accepted_table.set(i + 1, hex::encode(iface.as_bytes()))?;
                }
                result.set("accepted", accepted_table)?;

                let rejected_table = lua.create_table()?;
                for (i, (iface, _reason)) in ack.rejected.iter().enumerate() {
                    rejected_table.set(i + 1, hex::encode(iface.as_bytes()))?;
                }
                result.set("rejected", rejected_table)?;

                Ok(result)
            },
        );

        // -- store_event(tier_str, iface_hex, data_str) -> { accepted, reason? } --
        methods.add_async_method(
            "store_event",
            |lua, this, (tier_str, iface_hex, data_str): (String, String, String)| async move {
                let tier = parse_tier(&tier_str)?;
                let iface = hex_to_interface_id(&iface_hex)?;

                let mut guard = this.session.lock().await;
                let session = guard.as_mut().ok_or_else(|| {
                    mlua::Error::external("Session closed")
                })?;

                let ack = session
                    .store_event(tier, iface, data_str.into_bytes())
                    .await
                    .map_err(mlua::Error::external)?;

                let result = lua.create_table()?;
                result.set("accepted", ack.accepted)?;
                if let Some(reason) = ack.reason {
                    result.set("reason", reason)?;
                }

                Ok(result)
            },
        );

        // -- retrieve(iface_hex, tier_str?) -> { events, has_more } --
        methods.add_async_method(
            "retrieve",
            |lua, this, (iface_hex, tier_str): (String, Option<String>)| async move {
                let iface = hex_to_interface_id(&iface_hex)?;
                let tier = tier_str.as_deref().map(parse_tier).transpose()?;

                let mut guard = this.session.lock().await;
                let session = guard.as_mut().ok_or_else(|| {
                    mlua::Error::external("Session closed")
                })?;

                let delivery = session
                    .retrieve(iface, None, tier)
                    .await
                    .map_err(mlua::Error::external)?;

                let result = lua.create_table()?;

                let events_table = lua.create_table()?;
                for (i, stored) in delivery.events.iter().enumerate() {
                    let entry = lua.create_table()?;
                    entry.set(
                        "data",
                        String::from_utf8_lossy(&stored.encrypted_event).to_string(),
                    )?;
                    entry.set("event_id", stored.event_id.sequence)?;
                    events_table.set(i + 1, entry)?;
                }
                result.set("events", events_table)?;
                result.set("has_more", delivery.has_more)?;

                Ok(result)
            },
        );

        // -- sync_contacts(player_id_hexes_table) -> { accepted, contact_count } --
        methods.add_async_method(
            "sync_contacts",
            |lua, this, player_id_hexes_table: Table| async move {
                let mut contacts: Vec<[u8; 32]> = Vec::new();
                for pair in player_id_hexes_table.pairs::<mlua::Value, String>() {
                    let (_, hex_str) = pair?;
                    let bytes = hex::decode(&hex_str).map_err(|e| {
                        mlua::Error::external(format!("Invalid player ID hex: {}", e))
                    })?;
                    if bytes.len() != 32 {
                        return Err(mlua::Error::external("Player ID must be 32 bytes"));
                    }
                    let mut arr = [0u8; 32];
                    arr.copy_from_slice(&bytes);
                    contacts.push(arr);
                }

                let mut guard = this.session.lock().await;
                let session = guard.as_mut().ok_or_else(|| {
                    mlua::Error::external("Session closed")
                })?;

                let ack = session
                    .sync_contacts(contacts)
                    .await
                    .map_err(mlua::Error::external)?;

                let result = lua.create_table()?;
                result.set("accepted", ack.accepted)?;
                result.set("contact_count", ack.contact_count)?;

                Ok(result)
            },
        );

        // -- ping() -> rtt_ms: number --
        methods.add_async_method("ping", |_, this, ()| async move {
            let mut guard = this.session.lock().await;
            let session = guard.as_mut().ok_or_else(|| {
                mlua::Error::external("Session closed")
            })?;

            let rtt = session.ping().await.map_err(mlua::Error::external)?;
            Ok(rtt.as_millis() as f64)
        });

        // -- player_id() -> hex string --
        methods.add_method("player_id", |_, this, ()| {
            Ok(hex::encode(this.player_id))
        });

        // -- close() --
        methods.add_method_mut("close", |_, this, ()| {
            if let Ok(mut guard) = this.session.try_lock() {
                *guard = None;
            }
            Ok(())
        });

        // -- ToString --
        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            Ok(format!(
                "RelayClient(player_id={})",
                hex::encode(&this.player_id[..4])
            ))
        });
    }
}

// ---------------------------------------------------------------------------
// Internal constructor helper
// ---------------------------------------------------------------------------

async fn connect_client(
    signing_key: SigningKey,
    endpoint_addr: &iroh::EndpointAddr,
) -> mlua::Result<LuaRelayClient> {
    let transport_secret = SecretKey::generate(&mut rand::rng());
    let player_id = signing_key.verifying_key().to_bytes();
    let client = RelayClient::new(signing_key.clone(), transport_secret);
    let session = client
        .connect(endpoint_addr.clone())
        .await
        .map_err(mlua::Error::external)?;
    Ok(LuaRelayClient {
        session: Arc::new(Mutex::new(Some(session))),
        signing_key,
        player_id,
    })
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

/// Register RelayNode and RelayClient constructors with the `indras` Lua table.
pub fn register(lua: &Lua, indras: &Table) -> Result<()> {
    // -- RelayNode --
    let relay_node_table = lua.create_table()?;

    // RelayNode.new(config_table?) -> RelayNode
    //
    // Config fields (all optional):
    //   owner              (bool)   — generate a random owner signing key
    //   public_max_bytes   (u64)    — override Public tier quota
    //   cleanup_interval_secs (u64) — override cleanup interval
    relay_node_table.set(
        "new",
        lua.create_async_function(|_, config_table: Option<Table>| async move {
            let mut owner = false;
            let mut public_max_bytes: Option<u64> = None;
            let mut cleanup_interval_secs: Option<u64> = None;

            if let Some(ref t) = config_table {
                if let Ok(v) = t.get::<bool>("owner") {
                    owner = v;
                }
                if let Ok(v) = t.get::<u64>("public_max_bytes") {
                    public_max_bytes = Some(v);
                }
                if let Ok(v) = t.get::<u64>("cleanup_interval_secs") {
                    cleanup_interval_secs = Some(v);
                }
            }

            // Temp dir for relay data
            let tmp = tempfile::TempDir::new().map_err(mlua::Error::external)?;
            let data_path = tmp.path().to_path_buf();

            // Build config
            let mut config = RelayConfig::default();
            config.data_dir = data_path.clone();

            // Owner signing key
            let owner_signing_key = if owner {
                let sk = random_signing_key();
                config.owner_player_id = Some(hex::encode(sk.verifying_key().to_bytes()));
                Some(sk)
            } else {
                None
            };

            if let Some(bytes) = public_max_bytes {
                config.tiers.public_max_bytes = bytes;
            }
            if let Some(secs) = cleanup_interval_secs {
                config.storage.cleanup_interval_secs = secs;
            }

            // Construct and start the relay
            let relay = RelayNode::new(config).await.map_err(mlua::Error::external)?;
            let shutdown_token = relay.shutdown_token();
            let (endpoint_addr, handle) = relay.start().await.map_err(mlua::Error::external)?;

            // Wrap the cancel call in a closure so we don't store CancellationToken directly
            // (tokio-util is not a direct simulation dep)
            let shutdown_fn: Arc<dyn Fn() + Send + Sync> =
                Arc::new(move || shutdown_token.cancel());

            Ok(LuaRelayNode {
                shutdown_fn,
                endpoint_addr: Arc::new(endpoint_addr),
                data_dir: data_path,
                owner_signing_key,
                _temp_dir: Some(tmp),
                _handle: Some(handle),
            })
        })?,
    )?;

    indras.set("RelayNode", relay_node_table)?;

    // -- RelayClient --
    let relay_client_table = lua.create_table()?;

    // RelayClient.new(relay_node_ud) -> RelayClient
    //
    // Generates a fresh random signing key + transport key and connects.
    relay_client_table.set(
        "new",
        lua.create_async_function(|_, relay_node_ud: mlua::AnyUserData| async move {
            let endpoint_addr = {
                let node = relay_node_ud.borrow::<LuaRelayNode>()?;
                (*node.endpoint_addr).clone()
            };
            let signing_key = random_signing_key();
            connect_client(signing_key, &endpoint_addr).await
        })?,
    )?;

    // RelayClient.new_as_owner(relay_node_ud) -> RelayClient
    //
    // Reuses the relay's stored owner signing key so the player_id matches
    // the relay's owner_player_id. Errors if relay was not created with owner=true.
    relay_client_table.set(
        "new_as_owner",
        lua.create_async_function(|_, relay_node_ud: mlua::AnyUserData| async move {
            let (endpoint_addr, signing_key) = {
                let node = relay_node_ud.borrow::<LuaRelayNode>()?;
                let addr = (*node.endpoint_addr).clone();
                let sk = node
                    .owner_signing_key
                    .clone()
                    .ok_or_else(|| {
                        mlua::Error::external(
                            "Relay has no owner key; create RelayNode with owner=true",
                        )
                    })?;
                (addr, sk)
            };
            connect_client(signing_key, &endpoint_addr).await
        })?,
    )?;

    indras.set("RelayClient", relay_client_table)?;

    Ok(())
}
