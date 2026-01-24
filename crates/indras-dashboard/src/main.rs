use dioxus::desktop::tao::window::Icon;
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

/// App icon (loaded from assets at compile time)
const ICON_PNG: &[u8] = include_bytes!("../../../assets/Logo_black.png");

/// Load the app icon from embedded PNG data
fn load_icon() -> Option<Icon> {
    let image = image::load_from_memory(ICON_PNG).ok()?.into_rgba8();
    let (width, height) = image.dimensions();
    Icon::from_rgba(image.into_raw(), width, height).ok()
}

fn main() {
    // Initialize tracing for logging
    tracing_subscriber::fmt::init();

    // Load the app icon
    let icon = load_icon();

    // Launch Dioxus desktop app with custom CSS
    LaunchBuilder::desktop()
        .with_cfg(
            dioxus::desktop::Config::new()
                .with_window(
                    dioxus::desktop::WindowBuilder::new()
                        .with_title("IndrasNetwork Stress Test Dashboard")
                        .with_inner_size(dioxus::desktop::LogicalSize::new(1200.0, 800.0))
                        .with_maximized(true)
                        .with_window_icon(icon),
                )
                .with_custom_head(format!(
                    r#"<style>{}</style><style>{}</style>"#,
                    THEME_CSS, STYLE_CSS
                )),
        )
        .launch(app::App);
}
