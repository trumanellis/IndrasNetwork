//! Lua bindings for P2P vault sync (`indras-vault-sync`).
//!
//! Exposes `Vault` creation, joining, file operations, conflict listing,
//! and resolution to Lua scenarios for integration testing.

use mlua::{Lua, MetaMethod, Result, Table, UserData, UserDataMethods};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

use indras_network::IndrasNetwork;
use indras_vault_sync::realm_vault::RealmVault;
use indras_vault_sync::Vault;

/// Lua wrapper for a P2P-synced Vault.
///
/// Holds the vault in `Arc<Mutex<Option<Vault>>>` so that `stop()` can
/// take ownership (consuming the inner value) while other methods borrow it.
struct LuaVault {
    inner: Arc<Mutex<Option<Vault>>>,
    vault_path: PathBuf,
}

impl LuaVault {
    fn new(vault: Vault, vault_path: PathBuf) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Some(vault))),
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
            let files = vault
                .realm()
                .list_files()
                .await
                .map_err(mlua::Error::external)?;

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
            let conflicts = vault
                .realm()
                .list_conflicts()
                .await
                .map_err(mlua::Error::external)?;

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
                    .realm()
                    .resolve_conflict(&path, hash)
                    .await
                    .map_err(mlua::Error::external)?;
                Ok(())
            },
        );

        // -- write_file(rel_path, content_string) --
        // Test convenience: writes to disk, hashes with BLAKE3, stores in blob store, updates index.

        methods.add_async_method(
            "write_file",
            |_, this, (rel_path, content): (String, mlua::String)| async move {
                let data = content.as_bytes().to_vec();
                let guard = this.inner.lock().await;
                let vault = guard
                    .as_ref()
                    .ok_or_else(|| mlua::Error::external("Vault has been stopped"))?;

                // Write to disk
                let full_path = vault.path().join(&rel_path);
                if let Some(parent) = full_path.parent() {
                    tokio::fs::create_dir_all(parent)
                        .await
                        .map_err(mlua::Error::external)?;
                }
                tokio::fs::write(&full_path, &data)
                    .await
                    .map_err(mlua::Error::external)?;

                // Hash and update index
                let hash = *blake3::hash(&data).as_bytes();
                let size = data.len() as u64;
                let member_id = vault.member_id();

                vault
                    .realm()
                    .upsert_file(&rel_path, hash, size, member_id)
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

            // Remove from disk
            let full_path = vault.path().join(&rel_path);
            if full_path.exists() {
                tokio::fs::remove_file(&full_path)
                    .await
                    .map_err(mlua::Error::external)?;
            }

            // Mark deleted in index
            let member_id = vault.member_id();
            vault
                .realm()
                .delete_file(&rel_path, member_id)
                .await
                .map_err(mlua::Error::external)?;

            Ok(())
        });

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
                let (vault, invite) = Vault::create(&net_arc, &name, path.clone())
                    .await
                    .map_err(mlua::Error::external)?;

                let invite_str = invite.to_string();
                let lua_vault = LuaVault::new(vault, path);

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
                let vault = Vault::join(&net_arc, &invite_str, path.clone())
                    .await
                    .map_err(mlua::Error::external)?;

                Ok(LuaVault::new(vault, path))
            },
        )?,
    )?;

    indras.set("VaultSync", vault_table)?;

    Ok(())
}
