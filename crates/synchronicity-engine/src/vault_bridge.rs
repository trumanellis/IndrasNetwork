//! Bridge between the vault filesystem and the UI state.
//!
//! Handles scanning the vault directory for files and converting them into
//! [`FileView`] records for display in the file list panel.
//!
//! Also provides async functions for account creation and restore.

use std::sync::Arc;

use dioxus::prelude::*;
use indras_crypto::PassStory;
use indras_network::IndrasNetwork;

use crate::state::{AppState, AppStep, FileView, LoadingStage, SyncStatus, default_data_dir, format_relative_time};

/// HelloWorld.md content seeded into new vaults.
const HELLO_WORLD: &str = include_str!("../assets/HelloWorld.md");

/// Ensure the vault directory exists, initialize as Obsidian vault, and seed HelloWorld.md.
///
/// Called for returning users who skip the creation flow, and after account creation.
pub fn ensure_vault_ready(vault_path: &std::path::Path) {
    if let Err(e) = std::fs::create_dir_all(vault_path) {
        tracing::warn!("Failed to create vault directory: {e}");
        return;
    }
    // Initialize .obsidian directory so Obsidian recognizes this as a vault
    let obsidian_dir = vault_path.join(".obsidian");
    if !obsidian_dir.exists() {
        if let Err(e) = std::fs::create_dir_all(&obsidian_dir) {
            tracing::warn!("Failed to create .obsidian directory: {e}");
        }
    }
    let hello_path = vault_path.join("HelloWorld.md");
    if !hello_path.exists() {
        if let Err(e) = std::fs::write(&hello_path, HELLO_WORLD) {
            tracing::warn!("Failed to write HelloWorld.md: {e}");
        }
    }
}

/// Scan the vault directory and return a sorted list of file views.
///
/// Files are sorted by modification time, newest first.
pub fn scan_vault(vault_path: &std::path::Path) -> Vec<FileView> {
    let Ok(entries) = std::fs::read_dir(vault_path) else {
        return Vec::new();
    };

    let mut files: Vec<FileView> = entries
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            if !path.is_file() {
                return None;
            }
            let name = path.file_name()?.to_string_lossy().to_string();
            let meta = std::fs::metadata(&path).ok()?;
            let size = meta.len();
            let modified_ms = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);

            Some(FileView {
                path: name.clone(),
                name,
                size,
                modified: format_relative_time(modified_ms),
                modified_ms,
            })
        })
        .collect();

    files.sort_by(|a, b| b.modified_ms.cmp(&a.modified_ms));
    files
}

/// Create a new account: generate identity, start network, create vault dir, seed HelloWorld.md.
pub async fn create_account(
    mut state: Signal<AppState>,
    mut network: Signal<Option<Arc<IndrasNetwork>>>,
) {
    let data_dir = default_data_dir();
    let display_name = state.read().display_name.clone();
    let vault_path = state.read().vault_path.clone();

    // Stage 1: Create identity (plaintext keystore — pass story encryption deferred)
    state.write().loading_stages = vec![
        LoadingStage::InProgress("Creating identity...".into()),
    ];

    let net = match IndrasNetwork::builder()
        .data_dir(&data_dir)
        .display_name(&display_name)
        .build()
        .await
    {
        Ok(n) => n,
        Err(e) => {
            state.write().error = Some(format!("Identity creation failed: {e}"));
            return;
        }
    };

    state.write().loading_stages = vec![
        LoadingStage::Done("Identity created".into()),
        LoadingStage::InProgress("Starting network...".into()),
    ];

    // Stage 2: Start network
    if let Err(e) = net.start().await {
        state.write().error = Some(format!("Network start failed: {e}"));
        return;
    }

    state.write().loading_stages = vec![
        LoadingStage::Done("Identity created".into()),
        LoadingStage::Done("Network connected".into()),
        LoadingStage::InProgress("Creating vault...".into()),
    ];

    // Stage 3: Create vault directory, init Obsidian, seed HelloWorld.md
    ensure_vault_ready(&vault_path);

    state.write().loading_stages = vec![
        LoadingStage::Done("Identity created".into()),
        LoadingStage::Done("Network connected".into()),
        LoadingStage::Done("Vault ready".into()),
    ];

    network.set(Some(net));
    state.write().sync_status = SyncStatus::Synced;

    // Brief pause so user sees the success state
    tokio::time::sleep(std::time::Duration::from_millis(800)).await;

    state.write().step = AppStep::HomeVault;
}

/// Restore an existing account from pass story: derive identity, connect, sync vault.
pub async fn restore_account(
    mut state: Signal<AppState>,
    mut network: Signal<Option<Arc<IndrasNetwork>>>,
) {
    let data_dir = default_data_dir();
    let slots = state.read().pass_story_slots.clone();
    let vault_path = state.read().vault_path.clone();

    // Stage 1: Derive identity
    state.write().loading_stages = vec![
        LoadingStage::InProgress("Deriving identity from story...".into()),
    ];

    let slot_refs: Vec<&str> = slots.iter().map(|s| s.as_str()).collect();
    let slot_array: [&str; 23] = match slot_refs.try_into() {
        Ok(a) => a,
        Err(_) => {
            state.write().error = Some("Invalid pass story slots".into());
            return;
        }
    };

    let story = match PassStory::from_raw(&slot_array) {
        Ok(s) => s,
        Err(e) => {
            state.write().error = Some(format!("Pass story error: {e}"));
            return;
        }
    };

    let net = match IndrasNetwork::builder()
        .data_dir(&data_dir)
        .pass_story(story)
        .build()
        .await
    {
        Ok(n) => n,
        Err(e) => {
            state.write().error = Some(format!("Identity restore failed: {e}"));
            return;
        }
    };

    state.write().loading_stages = vec![
        LoadingStage::Done("Identity derived".into()),
        LoadingStage::InProgress("Connecting to network...".into()),
    ];

    // Stage 2: Start network
    if let Err(e) = net.start().await {
        state.write().error = Some(format!("Network start failed: {e}"));
        return;
    }

    state.write().loading_stages = vec![
        LoadingStage::Done("Identity derived".into()),
        LoadingStage::Done("Network connected".into()),
        LoadingStage::InProgress("Syncing vault...".into()),
    ];

    // Stage 3: Ensure vault directory exists
    if let Err(e) = std::fs::create_dir_all(&vault_path) {
        state.write().error = Some(format!("Failed to create vault directory: {e}"));
        return;
    }

    state.write().loading_stages = vec![
        LoadingStage::Done("Identity derived".into()),
        LoadingStage::Done("Network connected".into()),
        LoadingStage::Done("Vault syncing".into()),
    ];

    network.set(Some(net));
    state.write().sync_status = SyncStatus::Synced;

    tokio::time::sleep(std::time::Duration::from_millis(800)).await;

    state.write().step = AppStep::HomeVault;
}
