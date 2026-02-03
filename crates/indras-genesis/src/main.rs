//! Entry point for the Genesis flow.

use std::path::PathBuf;

use dioxus::desktop::{Config, LogicalPosition, LogicalSize, WindowBuilder};

use indras_genesis::components::App;

const SHARED_CSS: &str = indras_ui::SHARED_CSS;
const STYLES_CSS: &str = include_str!("../assets/styles.css");

/// Get the default data directory (mirrors app.rs logic).
fn default_data_dir() -> PathBuf {
    // If INDRAS_DATA_DIR is set, use it (for multi-instance mode)
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

    // Read name from env (set by ./se script or manually)
    let name = std::env::var("INDRAS_NAME").ok();

    // JSONL logging: pretty console + file output for diagnostics
    let log_prefix = match &name {
        Some(n) => format!("genesis-{}", n.to_lowercase()),
        None => "genesis".to_string(),
    };
    let _log_guard = indras_logging::IndrasSubscriberBuilder::new()
        .with_config(indras_logging::LogConfig {
            default_level: "debug".to_string(),
            console: indras_logging::ConsoleConfig {
                enabled: true,
                pretty: true,
                ansi: true,
                level: Some("info".to_string()),
            },
            file: Some(indras_logging::FileConfig {
                directory: std::path::PathBuf::from("./logs"),
                prefix: log_prefix,
                rotation: indras_logging::RotationStrategy::Never,
                max_files: None,
            }),
            ..Default::default()
        })
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
        Some(n) => format!("Synchronicity Engine - {}", n),
        None => "Synchronicity Engine".to_string(),
    };

    tracing::info!("Starting Genesis Flow");

    // Note: theme is set inside App component (GlobalSignal requires Dioxus runtime)

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
        wb = wb.with_inner_size(LogicalSize::new(900.0, 700.0));
    }

    if let (Some(x), Some(y)) = (win_x, win_y) {
        wb = wb.with_position(LogicalPosition::new(x, y));
    }

    dioxus::LaunchBuilder::desktop()
        .with_cfg(
            Config::new()
                .with_window(wb)
                .with_custom_head(format!(
                    r#"
                    <link rel="preconnect" href="https://fonts.googleapis.com">
                    <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
                    <link href="https://fonts.googleapis.com/css2?family=Cormorant+Garamond:wght@400;500;600;700&family=JetBrains+Mono:wght@400;500;600&display=swap" rel="stylesheet">
                    <style>{}</style>
                    <style>{}</style>
                    "#,
                    SHARED_CSS, STYLES_CSS
                )),
        )
        .launch(App);
}
