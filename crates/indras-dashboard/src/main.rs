use dioxus::prelude::*;

mod app;
mod components;
mod layout;
mod runner;
mod state;
pub mod theme;

/// Theme CSS (loaded from assets/themes.css at compile time)
const THEME_CSS: &str = include_str!("../assets/themes.css");

/// Component CSS (loaded from assets/style.css at compile time)
const STYLE_CSS: &str = include_str!("../assets/style.css");

fn main() {
    // Initialize tracing for logging
    tracing_subscriber::fmt::init();

    // Launch Dioxus desktop app with custom CSS
    LaunchBuilder::desktop()
        .with_cfg(
            dioxus::desktop::Config::new()
                .with_window(
                    dioxus::desktop::WindowBuilder::new()
                        .with_title("IndrasNetwork Stress Test Dashboard")
                        .with_inner_size(dioxus::desktop::LogicalSize::new(1200.0, 800.0))
                        .with_maximized(true),
                )
                .with_custom_head(format!(
                    r#"<style>{}</style><style>{}</style>"#,
                    THEME_CSS, STYLE_CSS
                )),
        )
        .launch(app::App);
}
