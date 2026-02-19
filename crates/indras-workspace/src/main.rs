//! Entry point for the Indras Workspace desktop app.

use std::path::PathBuf;
#[cfg(feature = "lua-scripting")]
use std::sync::Arc;

use dioxus::desktop::{Config, LogicalPosition, LogicalSize, WindowBuilder};

use indras_workspace::components::app::RootApp;
use indras_ui::ThemedRoot;

#[cfg(feature = "lua-scripting")]
use indras_workspace::scripting::channels::{create_test_channels, AppTestChannels};
#[cfg(feature = "lua-scripting")]
use indras_workspace::scripting::lua_runtime::LuaTestRuntime;

/// Workspace-specific CSS embedded at compile time.
const WORKSPACE_CSS: &str = include_str!("../assets/workspace.css");

/// Get the default data directory (mirrors network_bridge.rs logic).
fn default_data_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("INDRAS_DATA_DIR") {
        return PathBuf::from(dir);
    }

    #[cfg(target_os = "macos")]
    {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join("Library/Application Support/indras-network");
        }
    }
    #[cfg(target_os = "linux")]
    {
        if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
            return PathBuf::from(xdg).join("indras-network");
        }
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join(".local/share/indras-network");
        }
    }
    #[cfg(target_os = "windows")]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            return PathBuf::from(appdata).join("indras-network");
        }
    }
    PathBuf::from(".").join("indras-network")
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let clean = args.iter().any(|a| a == "--clean");
    let mock = args.iter().any(|a| a == "--mock");
    *indras_workspace::MOCK_ARTIFACTS.lock().unwrap() = mock;

    let name = std::env::var("INDRAS_NAME").ok();

    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .init();

    if clean {
        let data_dir = default_data_dir();
        if data_dir.exists() {
            tracing::info!("--clean: removing user data at {}", data_dir.display());
            if let Err(e) = std::fs::remove_dir_all(&data_dir) {
                tracing::error!("Failed to remove data directory: {}", e);
            } else {
                tracing::info!("User data removed successfully");
            }
        } else {
            tracing::info!("--clean: no data directory found at {}", data_dir.display());
        }
    }

    let window_title = match &name {
        Some(n) => format!("Indras Workspace - {}", n),
        None => "Indras Workspace".to_string(),
    };

    tracing::info!("Starting Indras Workspace");

    // --- Lua scripting support (feature-gated) ---
    #[cfg(feature = "lua-scripting")]
    let test_channels: Option<Arc<tokio::sync::Mutex<AppTestChannels>>> = {
        // Only allow in debug builds
        #[cfg(not(debug_assertions))]
        {
            eprintln!("WARNING: Lua scripting is only available in debug builds");
            None
        }
        #[cfg(debug_assertions)]
        {
            let script_path = args
                .iter()
                .find(|a| a.starts_with("--script="))
                .map(|a| a.trim_start_matches("--script=").to_string());

            if let Some(path) = script_path {
                tracing::info!("Lua script: {}", path);
                let (app_channels, lua_channels) = create_test_channels();
                let identity_name = name.clone();

                // Parse optional script timeout (default: 120s)
                let timeout_secs: u64 = args.iter()
                    .find(|a| a.starts_with("--timeout="))
                    .and_then(|a| a.trim_start_matches("--timeout=").parse().ok())
                    .unwrap_or(120);

                // Spawn timeout watchdog thread
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_secs(timeout_secs));
                    tracing::error!("Script timeout after {}s", timeout_secs);
                    std::process::exit(2); // Exit code 2 = timeout
                });

                // Spawn Lua thread (mlua's Lua is !Send, needs dedicated OS thread)
                std::thread::spawn(move || {
                    let rt = LuaTestRuntime::new(
                        lua_channels.action_tx,
                        lua_channels.event_rx,
                        lua_channels.query_tx,
                        identity_name,
                    );
                    match rt.exec_file(&path) {
                        Ok(()) => {
                            tracing::info!("Lua script completed successfully");
                            std::process::exit(0); // 0 = pass
                        }
                        Err(e) => {
                            let msg = e.to_string();
                            if msg.contains("Assertion failed") {
                                tracing::error!("Assertion failure: {}", msg);
                                std::process::exit(1); // 1 = assertion failure
                            } else if msg.contains("Timeout") {
                                tracing::error!("Script timeout: {}", msg);
                                std::process::exit(2); // 2 = timeout
                            } else {
                                tracing::error!("Script error: {}", msg);
                                std::process::exit(3); // 3 = runtime error
                            }
                        }
                    }
                });

                Some(Arc::new(tokio::sync::Mutex::new(app_channels)))
            } else {
                None
            }
        }
    };

    // Read optional window geometry from env (set by ./se for tiling)
    let win_x = std::env::var("INDRAS_WIN_X").ok().and_then(|v| v.parse::<f64>().ok());
    let win_y = std::env::var("INDRAS_WIN_Y").ok().and_then(|v| v.parse::<f64>().ok());
    let win_w = std::env::var("INDRAS_WIN_W").ok().and_then(|v| v.parse::<f64>().ok());
    let win_h = std::env::var("INDRAS_WIN_H").ok().and_then(|v| v.parse::<f64>().ok());

    let mut wb = WindowBuilder::new()
        .with_title(&window_title)
        .with_maximized(false);

    if let (Some(w), Some(h)) = (win_w, win_h) {
        wb = wb.with_inner_size(LogicalSize::new(w, h));
    } else {
        wb = wb.with_inner_size(LogicalSize::new(1400.0, 900.0));
    }

    if let (Some(x), Some(y)) = (win_x, win_y) {
        wb = wb.with_position(LogicalPosition::new(x, y));
    }

    // Store test channels for the Dioxus App component to pick up
    #[cfg(feature = "lua-scripting")]
    if let Some(channels) = test_channels {
        *TEST_CHANNELS.lock().unwrap() = Some(channels);
    }

    dioxus::LaunchBuilder::desktop()
        .with_cfg(
            Config::new()
                .with_window(wb)
                .with_custom_head(format!(
                    r#"
                    <link rel="preconnect" href="https://fonts.googleapis.com">
                    <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
                    <link href="https://fonts.googleapis.com/css2?family=DM+Sans:ital,wght@0,300;0,400;0,500;0,600;0,700;1,400&family=JetBrains+Mono:wght@300;400;500&display=swap" rel="stylesheet">
                    <style>{}</style>
                    <style>{}</style>
                    "#,
                    indras_ui::SHARED_CSS,
                    WORKSPACE_CSS,
                )),
        )
        .launch(App);
}

use dioxus::prelude::*;

#[component]
fn App() -> Element {
    // Provide test channels to the component tree if lua-scripting is enabled
    #[cfg(feature = "lua-scripting")]
    {
        // This is set as a static in main() and read here.
        // We use use_hook to provide it once.
        use_context_provider(|| TEST_CHANNELS.lock().unwrap().take());
    }

    rsx! {
        ThemedRoot {
            RootApp {}
        }
    }
}

#[cfg(feature = "lua-scripting")]
static TEST_CHANNELS: std::sync::Mutex<Option<Arc<tokio::sync::Mutex<AppTestChannels>>>> =
    std::sync::Mutex::new(None);

