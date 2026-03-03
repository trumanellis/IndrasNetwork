//! Entry point for the Gift Cycle desktop app.

use std::path::PathBuf;

use dioxus::desktop::{Config, LogicalPosition, LogicalSize, WindowBuilder};
use dioxus::prelude::*;

mod app;
mod bridge;
mod components;
mod data;
mod state;

/// Gift Cycle CSS embedded at compile time.
const GIFT_CYCLE_CSS: &str = include_str!("../assets/gift-cycle.css");

/// Get the default data directory (mirrors indras-workspace logic).
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
    let name = std::env::var("INDRAS_NAME").ok();

    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .init();

    if clean {
        let data_dir = default_data_dir();
        if data_dir.exists() {
            tracing::info!("--clean: removing user data at {}", data_dir.display());
            let _ = std::fs::remove_dir_all(&data_dir);
        }
    }

    let window_title = match &name {
        Some(n) => format!("Gift Cycle \u{2014} {}", n),
        None => "Gift Cycle".to_string(),
    };

    tracing::info!("Starting Gift Cycle");

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
        wb = wb.with_inner_size(LogicalSize::new(1200.0, 800.0));
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
                    <link href="https://fonts.googleapis.com/css2?family=Inter:wght@300;400;500;600;700;800&family=JetBrains+Mono:wght@300;400;500&display=swap" rel="stylesheet">
                    <style>{}</style>
                    "#,
                    GIFT_CYCLE_CSS,
                )),
        )
        .launch(App);
}

#[component]
fn App() -> Element {
    rsx! {
        app::GiftCycleApp {}
    }
}
