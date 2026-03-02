//! Identity creation and loading boot sequence.
//!
//! Extracts the first-run check, identity creation/loading, network start,
//! contacts realm join, and home realm initialization into a standalone
//! async function that returns all the handles the app needs.

use std::sync::Arc;
use tokio::sync::Mutex;

use crate::bridge::network_bridge::{NetworkHandle, is_first_run, create_identity, load_identity};
use crate::bridge::vault_bridge::{VaultHandle, InMemoryVault};

/// Result of a successful boot sequence.
pub struct BootResult {
    /// The loaded network handle.
    pub network_handle: NetworkHandle,
    /// The vault handle for local artifact storage.
    pub vault_handle: VaultHandle,
    /// The home realm for persistent artifact storage (if initialization succeeded).
    pub home_realm: Option<indras_network::HomeRealm>,
    /// Log messages generated during boot for the event log.
    pub log_messages: Vec<String>,
}

/// Errors that can occur during the boot sequence.
#[derive(Debug)]
pub enum BootError {
    /// No identity exists and no INDRAS_NAME env var is set; show setup UI.
    NeedsSetup,
    /// Auto-creation from INDRAS_NAME failed.
    AutoCreateFailed(String),
    /// Identity loading failed.
    LoadFailed(String),
    /// Vault creation failed.
    VaultFailed(String),
}

/// Run the identity boot sequence: check first-run, create/load identity,
/// start the network, join contacts realm, and initialize home realm.
///
/// Returns a `BootResult` on success or a `BootError` explaining what
/// went wrong and whether the setup UI should be shown.
pub async fn run_boot_sequence() -> Result<BootResult, BootError> {
    let mut log_messages = Vec::new();

    // Check if this is a first run
    if is_first_run() {
        // Auto-create identity if INDRAS_NAME is set (e.g., --remock)
        if let Ok(auto_name) = std::env::var("INDRAS_NAME") {
            match create_identity(&auto_name, None).await {
                Ok(_) => {
                    tracing::info!("Auto-created identity: {}", auto_name);
                    // Fall through to load_identity below
                }
                Err(e) => {
                    tracing::error!("Auto-create identity failed: {}", e);
                    return Err(BootError::AutoCreateFailed(e));
                }
            }
        } else {
            return Err(BootError::NeedsSetup);
        }
    }

    // Load identity (works for both returning user and auto-created)
    let nh = match load_identity().await {
        Ok(nh) => nh,
        Err(e) => {
            tracing::error!("Failed to load identity: {}", e);
            return Err(BootError::LoadFailed(format!("Failed to load identity: {}", e)));
        }
    };

    let player_name = nh.network.display_name()
        .unwrap_or("Unknown").to_string();
    let player_id = nh.network.id();

    let now = chrono::Utc::now().timestamp_millis();
    let vault = match InMemoryVault::in_memory(player_id, now) {
        Ok(v) => v,
        Err(e) => {
            tracing::error!("Vault creation failed: {}", e);
            return Err(BootError::VaultFailed(format!("Vault creation failed: {}", e)));
        }
    };

    let vault_handle = VaultHandle {
        vault: Arc::new(Mutex::new(vault)),
        player_id,
        player_name: player_name.clone(),
    };

    log_messages.push(format!("Identity loaded: {}", player_name));

    let net = Arc::clone(&nh.network);

    // Start the network (enables inbox listener for incoming connections)
    log_messages.push("Starting network...".to_string());
    if let Err(e) = net.start().await {
        tracing::warn!(error = %e, "Failed to start network (non-fatal)");
        log_messages.push(format!("Network start warning: {}", e));
    } else {
        log_messages.push("Network started \u{2014} listening for connections".to_string());
    }

    // Join contacts realm so inbox listener can store contacts
    if let Err(e) = net.join_contacts_realm().await {
        tracing::warn!(error = %e, "Failed to join contacts realm (non-fatal)");
    }

    // Initialize home realm for persistent artifact storage
    let home_realm = match net.home_realm().await {
        Ok(hr) => {
            log_messages.push("Home realm initialized".to_string());
            Some(hr)
        }
        Err(e) => {
            tracing::warn!(error = %e, "Failed to initialize home realm (non-fatal)");
            log_messages.push(format!("Home realm warning: {}", e));
            None
        }
    };

    Ok(BootResult {
        network_handle: nh,
        vault_handle,
        home_realm,
        log_messages,
    })
}
