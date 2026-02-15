//! Entry point for the Indras Workspace desktop app.

use dioxus::desktop::{Config, LogicalSize, WindowBuilder};

use indras_workspace::components::app::RootApp;
use indras_ui::ThemedRoot;

/// Workspace-specific CSS embedded at compile time.
const WORKSPACE_CSS: &str = include_str!("../assets/workspace.css");

fn main() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .init();

    tracing::info!("Starting Indras Workspace");

    dioxus::LaunchBuilder::desktop()
        .with_cfg(
            Config::new()
                .with_window(
                    WindowBuilder::new()
                        .with_title("Indras Workspace")
                        .with_inner_size(LogicalSize::new(1400, 900)),
                )
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
    rsx! {
        ThemedRoot {
            RootApp {}
        }
    }
}
