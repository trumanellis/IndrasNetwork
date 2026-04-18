//! Entry point for The Synchronicity Engine desktop app.

use dioxus::desktop::{Config, LogicalPosition, LogicalSize, WindowBuilder};

use indras_logging::{ConsoleConfig, FileConfig, IndrasSubscriberBuilder, LogConfig, RotationStrategy};
use synchronicity_engine::components::App;

const SHARED_CSS: &str = indras_ui::SHARED_CSS;
const STYLES_CSS: &str = include_str!("../assets/styles.css");
const MILKDOWN_JS: &str = include_str!("../assets/milkdown-bundle.js");

fn main() {
    // JSONL log file under the per-instance data dir so Love/Joy/Peace don't
    // clobber each other's logs. Console stays on for dev visibility.
    let log_dir = synchronicity_engine::state::default_data_dir().join("logs");
    let log_config = LogConfig {
        default_level: "synchronicity_engine=info,indras_network=info,indras_node=info,indras_sync_engine=info,warn".to_string(),
        console: ConsoleConfig {
            enabled: true,
            pretty: true,
            ansi: true,
            level: None,
        },
        file: Some(FileConfig {
            directory: log_dir.clone(),
            prefix: "synchronicity-engine".to_string(),
            rotation: RotationStrategy::Never,
            max_files: None,
        }),
        ..LogConfig::default()
    };
    let _log_guard = IndrasSubscriberBuilder::new()
        .with_config(log_config)
        .init();
    tracing::info!(log_dir = %log_dir.display(), "JSONL log writer initialized");

    let name = std::env::var("INDRAS_NAME").ok();

    let window_title = match &name {
        Some(n) => format!("The Synchronicity Engine - {}", n),
        None => "The Synchronicity Engine".to_string(),
    };

    tracing::info!("Starting {}", window_title);

    // Read optional window geometry from env (set by ./se for tiling)
    let win_x = std::env::var("INDRAS_WIN_X").ok().and_then(|v| v.parse::<f64>().ok());
    let win_y = std::env::var("INDRAS_WIN_Y").ok().and_then(|v| v.parse::<f64>().ok());
    let win_w = std::env::var("INDRAS_WIN_W").ok().and_then(|v| v.parse::<f64>().ok());
    let win_h = std::env::var("INDRAS_WIN_H").ok().and_then(|v| v.parse::<f64>().ok());

    let tiling = win_w.is_some() || win_x.is_some();

    let mut wb = WindowBuilder::new()
        .with_title(&window_title)
        .with_maximized(!tiling)
        .with_focused(true)
        .with_always_on_top(false);

    if let (Some(w), Some(h)) = (win_w, win_h) {
        wb = wb.with_inner_size(LogicalSize::new(w, h));
    } else {
        wb = wb.with_inner_size(LogicalSize::new(1100.0, 750.0));
    }

    if let (Some(x), Some(y)) = (win_x, win_y) {
        wb = wb.with_position(LogicalPosition::new(x, y));
    }

    dioxus::LaunchBuilder::desktop()
        .with_cfg(
            Config::new()
                .with_window(wb)
                .with_custom_head(format!(
                    r#"<style>{}</style><style>{}</style><script>{}</script>"#,
                    SHARED_CSS, STYLES_CSS, MILKDOWN_JS
                )),
        )
        .launch(App);
}
