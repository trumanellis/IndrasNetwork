//! Entry point for the Indras Chat desktop app.

use dioxus::desktop::{Config, LogicalPosition, LogicalSize, WindowBuilder};

mod bridge;
mod components;
mod state;

const CHAT_CSS: &str = include_str!("style.css");

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter("indras_chat=info,indras_network=info")
        .init();

    let name = std::env::var("INDRAS_NAME").ok();

    let window_title = match &name {
        Some(n) => format!("Indras Chat - {}", n),
        None => "Indras Chat".to_string(),
    };

    tracing::info!("Starting {}", window_title);

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
        wb = wb.with_inner_size(LogicalSize::new(900.0, 600.0));
    }

    if let (Some(x), Some(y)) = (win_x, win_y) {
        wb = wb.with_position(LogicalPosition::new(x, y));
    }

    dioxus::LaunchBuilder::desktop()
        .with_cfg(
            Config::new()
                .with_window(wb)
                .with_custom_head(format!(
                    r#"<style>{}</style>"#,
                    CHAT_CSS,
                )),
        )
        .launch(components::app::App);
}
