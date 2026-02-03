//! Entry point for the Genesis flow.

use std::path::PathBuf;

use dioxus::desktop::{Config, LogicalSize, WindowBuilder};

use indras_genesis::components::App;

const STYLES_CSS: &str = include_str!("../assets/styles.css");

/// Get the default data directory (mirrors app.rs logic).
fn default_data_dir() -> PathBuf {
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

    tracing::info!("Starting Genesis Flow");

    dioxus::LaunchBuilder::desktop()
        .with_cfg(
            Config::new()
                .with_window(
                    WindowBuilder::new()
                        .with_title("Genesis - Indras Network")
                        .with_inner_size(LogicalSize::new(900.0, 700.0))
                        .with_maximized(true),
                )
                .with_custom_head(format!(
                    r#"
                    <link rel="preconnect" href="https://fonts.googleapis.com">
                    <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
                    <link href="https://fonts.googleapis.com/css2?family=Cormorant+Garamond:wght@400;500;600;700&family=JetBrains+Mono:wght@400;500;600&display=swap" rel="stylesheet">
                    <style>{}</style>
                    "#,
                    STYLES_CSS
                )),
        )
        .launch(App);
}
