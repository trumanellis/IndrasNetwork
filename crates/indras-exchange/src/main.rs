use dioxus::desktop::{Config, LogicalSize, WindowBuilder};
use indras_exchange::app::App;

const SHARED_CSS: &str = indras_ui::SHARED_CSS;
const EXCHANGE_CSS: &str = include_str!("../assets/exchange.css");

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .init();

    tracing::info!("Starting Intention Exchange");

    let wb = WindowBuilder::new()
        .with_title("Intention Exchange")
        .with_inner_size(LogicalSize::new(900.0, 700.0));

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
                    SHARED_CSS, EXCHANGE_CSS
                )),
        )
        .launch(App);
}
