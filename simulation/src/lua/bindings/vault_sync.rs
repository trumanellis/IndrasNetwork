//! Lua bindings for P2P vault sync (`indras-vault-sync`).
//!
//! Exposes `Vault` creation, joining, file operations, conflict listing,
//! and resolution to Lua scenarios for integration testing.

use mlua::{Lua, MetaMethod, Result, Table, UserData, UserDataMethods};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

use indras_network::IndrasNetwork;
use indras_storage::{BlobStore, BlobStoreConfig};
use indras_sync_engine::vault::Vault;

/// Lua wrapper for a P2P-synced Vault.
///
/// Holds the vault in `Arc<Mutex<Option<Vault>>>` so that `stop()` can
/// take ownership (consuming the inner value) while other methods borrow it.
struct LuaVault {
    inner: Arc<Mutex<Option<Vault>>>,
    network: Arc<IndrasNetwork>,
    vault_path: PathBuf,
}

impl LuaVault {
    fn new(vault: Vault, network: Arc<IndrasNetwork>, vault_path: PathBuf) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Some(vault))),
            network,
            vault_path,
        }
    }
}

impl UserData for LuaVault {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // -- scan() -> number of files indexed --

        methods.add_async_method("scan", |_, this, ()| async move {
            let guard = this.inner.lock().await;
            let vault = guard
                .as_ref()
                .ok_or_else(|| mlua::Error::external("Vault has been stopped"))?;
            let count = vault.initial_scan().await.map_err(mlua::Error::external)?;
            Ok(count)
        });

        // -- list_files() -> table of {path, hash_hex, size, modified_ms, deleted} --

        methods.add_async_method("list_files", |lua, this, ()| async move {
            let guard = this.inner.lock().await;
            let vault = guard
                .as_ref()
                .ok_or_else(|| mlua::Error::external("Vault has been stopped"))?;
            let files = vault.list_files().await;

            let result = lua.create_table()?;
            for (i, f) in files.iter().enumerate() {
                let entry = lua.create_table()?;
                entry.set("path", f.path.as_str())?;
                entry.set("hash_hex", hex::encode(f.hash))?;
                entry.set("size", f.size)?;
                entry.set("modified_ms", f.modified_ms)?;
                entry.set("deleted", f.deleted)?;
                result.set(i + 1, entry)?;
            }
            Ok(result)
        });

        // -- list_conflicts() -> table of {path, winner_hash, loser_hash, conflict_file, resolved} --

        methods.add_async_method("list_conflicts", |lua, this, ()| async move {
            let guard = this.inner.lock().await;
            let vault = guard
                .as_ref()
                .ok_or_else(|| mlua::Error::external("Vault has been stopped"))?;
            let conflicts = vault.list_conflicts().await;

            let result = lua.create_table()?;
            for (i, c) in conflicts.iter().enumerate() {
                let entry = lua.create_table()?;
                entry.set("path", c.path.as_str())?;
                entry.set("winner_hash", hex::encode(c.winner_hash))?;
                entry.set("loser_hash", hex::encode(c.loser_hash))?;
                entry.set("conflict_file", c.conflict_filename())?;
                entry.set("resolved", c.resolved)?;
                result.set(i + 1, entry)?;
            }
            Ok(result)
        });

        // -- resolve_conflict(path, loser_hash_hex) --

        methods.add_async_method(
            "resolve_conflict",
            |_, this, (path, loser_hash_hex): (String, String)| async move {
                let loser_bytes = hex::decode(&loser_hash_hex)
                    .map_err(|e| mlua::Error::external(format!("Invalid loser hash hex: {}", e)))?;
                if loser_bytes.len() != 32 {
                    return Err(mlua::Error::external("Loser hash must be 32 bytes"));
                }
                let mut hash = [0u8; 32];
                hash.copy_from_slice(&loser_bytes);

                let guard = this.inner.lock().await;
                let vault = guard
                    .as_ref()
                    .ok_or_else(|| mlua::Error::external("Vault has been stopped"))?;
                vault
                    .resolve_conflict(&path, &hash)
                    .await
                    .map_err(mlua::Error::external)?;
                Ok(())
            },
        );

        // -- write_file(rel_path, content_string) --
        // Writes to disk, stores blob, updates CRDT index, and suppresses watcher echo.

        methods.add_async_method(
            "write_file",
            |_, this, (rel_path, content): (String, mlua::String)| async move {
                let data = content.as_bytes().to_vec();
                let guard = this.inner.lock().await;
                let vault = guard
                    .as_ref()
                    .ok_or_else(|| mlua::Error::external("Vault has been stopped"))?;

                vault
                    .write_file_content(&rel_path, &data)
                    .await
                    .map_err(mlua::Error::external)?;

                Ok(())
            },
        );

        // -- delete_file(rel_path) --

        methods.add_async_method("delete_file", |_, this, rel_path: String| async move {
            let guard = this.inner.lock().await;
            let vault = guard
                .as_ref()
                .ok_or_else(|| mlua::Error::external("Vault has been stopped"))?;

            vault
                .delete_file_content(&rel_path)
                .await
                .map_err(mlua::Error::external)?;

            Ok(())
        });

        // -- add_peer_relay(other_network) -> bool --
        // Add a peer's relay for blob replication (in-process, needs userdata ref).

        methods.add_async_method(
            "add_peer_relay",
            |_, this, other_ud: mlua::AnyUserData| async move {
                let peer_addr = {
                    let other_ref = other_ud.borrow::<super::live_network::LuaNetwork>()?;
                    other_ref
                        .network
                        .endpoint_addr()
                        .await
                        .ok_or_else(|| mlua::Error::external("Peer network not started"))?
                };

                let guard = this.inner.lock().await;
                let vault = guard
                    .as_ref()
                    .ok_or_else(|| mlua::Error::external("Vault has been stopped"))?;
                let result = vault.add_peer_relay(&this.network, peer_addr).await;
                Ok(result)
            },
        );

        // -- add_peer_relay_addr(addr_string) -> bool --
        // Add a peer's relay via serialized endpoint address (cross-process).

        methods.add_async_method(
            "add_peer_relay_addr",
            |_, this, addr_str: String| async move {
                use base64::Engine;
                let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
                    .decode(&addr_str)
                    .map_err(|e| mlua::Error::external(format!("Invalid base64 addr: {e}")))?;
                let peer_addr: iroh::EndpointAddr = postcard::from_bytes(&bytes)
                    .map_err(|e| mlua::Error::external(format!("Invalid endpoint addr: {e}")))?;

                let guard = this.inner.lock().await;
                let vault = guard
                    .as_ref()
                    .ok_or_else(|| mlua::Error::external("Vault has been stopped"))?;
                let result = vault.add_peer_relay(&this.network, peer_addr).await;
                Ok(result)
            },
        );

        // -- await_members(expected_count, timeout_secs) -> actual_count --
        // Polls realm member count until expected is reached or timeout expires.

        methods.add_async_method(
            "await_members",
            |_, this, (expected, timeout_secs): (usize, f64)| async move {
                let guard = this.inner.lock().await;
                let vault = guard
                    .as_ref()
                    .ok_or_else(|| mlua::Error::external("Vault has been stopped"))?;
                let timeout = std::time::Duration::from_secs_f64(timeout_secs);
                let count = vault
                    .await_members(expected, timeout)
                    .await
                    .map_err(mlua::Error::external)?;
                Ok(count)
            },
        );

        // -- vault_path() -> string --

        methods.add_method("vault_path", |_, this, ()| {
            Ok(this.vault_path.to_string_lossy().to_string())
        });

        // -- stop() --

        methods.add_async_method("stop", |_, this, ()| async move {
            let mut guard = this.inner.lock().await;
            if let Some(vault) = guard.take() {
                vault.stop();
            }
            Ok(())
        });

        // -- ToString --

        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            Ok(format!("Vault(path={})", this.vault_path.display()))
        });
    }
}

/// Register VaultSync bindings with the indras Lua table.
pub fn register(lua: &Lua, indras: &Table) -> Result<()> {
    let vault_table = lua.create_table()?;

    // VaultSync.create(network_userdata, name, vault_path) -> (LuaVault, invite_string)
    vault_table.set(
        "create",
        lua.create_async_function(
            |_, (network_ud, name, vault_path): (mlua::AnyUserData, String, String)| async move {
                // Extract Arc<IndrasNetwork> from the LuaNetwork userdata.
                // LuaNetwork has a field `network: Arc<IndrasNetwork>`.
                // We access it via the borrow API.
                let net_arc: Arc<IndrasNetwork> = {
                    // The LuaNetwork struct is private to live_network module, but we can
                    // call a method on the userdata to get what we need. Instead, we'll
                    // use a helper approach: call the userdata method.
                    //
                    // Actually, we need to work around the private struct. Let's use
                    // mlua's ability to get a field from UserData via a method call.
                    // We can't borrow LuaNetwork directly from here since it's private.
                    //
                    // The cleanest approach: store the Arc<IndrasNetwork> as Lua app data
                    // or use a public accessor. For now, we'll use the AnyUserData's
                    // named_user_value approach if available, or we'll make the network
                    // accessible via a registered user value.
                    //
                    // Simplest approach: we'll accept a path string instead of network
                    // userdata, or we make LuaNetwork pub.
                    //
                    // Best approach for this binding: accept the network userdata and
                    // use mlua's scope to extract the inner value. Since LuaNetwork wraps
                    // Arc<IndrasNetwork>, and we're in the same crate, we can make the
                    // struct field pub(crate).
                    //
                    // For now, let's reference the field directly since we're in the
                    // same crate (simulation).
                    let net_ref = network_ud
                        .borrow::<super::live_network::LuaNetwork>()?;
                    Arc::clone(&net_ref.network)
                };

                let path = PathBuf::from(&vault_path);
                let blob_store = create_blob_store(&path).await.map_err(mlua::Error::external)?;
                let (vault, invite) = Vault::create(&net_arc, &name, path.clone(), blob_store)
                    .await
                    .map_err(mlua::Error::external)?;

                let invite_str = invite.to_string();
                let lua_vault = LuaVault::new(vault, net_arc, path);

                Ok((lua_vault, invite_str))
            },
        )?,
    )?;

    // VaultSync.join(network_userdata, invite_string, vault_path) -> LuaVault
    vault_table.set(
        "join",
        lua.create_async_function(
            |_, (network_ud, invite_str, vault_path): (mlua::AnyUserData, String, String)| async move {
                let net_arc: Arc<IndrasNetwork> = {
                    let net_ref = network_ud
                        .borrow::<super::live_network::LuaNetwork>()?;
                    Arc::clone(&net_ref.network)
                };

                let path = PathBuf::from(&vault_path);
                let blob_store = create_blob_store(&path).await.map_err(mlua::Error::external)?;
                let vault = Vault::join(&net_arc, &invite_str, path.clone(), blob_store)
                    .await
                    .map_err(mlua::Error::external)?;

                Ok(LuaVault::new(vault, net_arc, path))
            },
        )?,
    )?;

    indras.set("VaultSync", vault_table)?;

    Ok(())
}

/// Create a blob store under `vault_path/.indras/blobs/`.
///
/// In the Lua test harness each vault gets its own store (tests run
/// in isolated temp dirs). The production app uses a shared store via
/// `VaultManager` instead.
async fn create_blob_store(vault_path: &PathBuf) -> std::result::Result<Arc<BlobStore>, String> {
    let blob_dir = vault_path.join(".indras/blobs");
    let config = BlobStoreConfig {
        base_dir: blob_dir,
        ..Default::default()
    };
    let store = BlobStore::new(config)
        .await
        .map_err(|e| format!("blob store: {e}"))?;
    Ok(Arc::new(store))
}
