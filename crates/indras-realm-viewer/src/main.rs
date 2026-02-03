//! Indras Realm Viewer - Main entry point
//!
//! A standalone Dioxus desktop application for visualizing realm features.
//!
//! Usage:
//!   lua_runner scenario.lua | realm-viewer
//!   realm-viewer --file events.jsonl

use std::path::PathBuf;
use std::sync::OnceLock;

use clap::Parser;
use dioxus::prelude::*;

use indras_realm_viewer::components::App;
use indras_realm_viewer::events::{start_stream, StreamConfig, StreamEvent};
use indras_realm_viewer::playback;
use indras_realm_viewer::state::{event_buffer, AppState};

/// Embedded CSS styles
const SHARED_CSS: &str = indras_ui::SHARED_CSS;
const STYLES_CSS: &str = include_str!("../assets/styles.css");

/// Global file path for stream config
static FILE_PATH: OnceLock<Option<PathBuf>> = OnceLock::new();

/// Command-line arguments
#[derive(Parser, Debug)]
#[command(name = "realm-viewer")]
#[command(about = "Visualize Indras realm features from streaming events")]
struct Args {
    /// Read events from file instead of stdin
    #[arg(short, long)]
    file: Option<PathBuf>,

    /// Initial theme (quiet-protocol or light)
    #[arg(short, long, default_value = "quiet-protocol")]
    theme: String,
}

fn main() {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .init();

    let args = Args::parse();

    // Store file path in global
    FILE_PATH.set(args.file).ok();

    // Set initial theme
    if args.theme == "light" {
        *indras_realm_viewer::theme::CURRENT_THEME.write() =
            indras_realm_viewer::theme::Theme::Light;
    }

    // Launch the desktop app
    dioxus::LaunchBuilder::desktop()
        .with_cfg(
            dioxus::desktop::Config::new()
                .with_window(
                    dioxus::desktop::WindowBuilder::new()
                        .with_title("Realm Viewer - Indras Network")
                        .with_inner_size(dioxus::desktop::LogicalSize::new(1400, 900))
                        .with_resizable(true)
                        .with_maximized(true),
                )
                .with_custom_head(format!(r#"<style>{}</style><style>{}</style>"#, SHARED_CSS, STYLES_CSS)),
        )
        .launch(RootApp);
}

/// Root application component
fn RootApp() -> Element {
    // Create app state signal
    let state = use_signal(AppState::new);

    // Request shutdown when component unmounts (window closing)
    use_drop(|| {
        playback::request_shutdown();
    });

    // Start the stream reader once
    let _stream_handle = use_resource(move || {
        let mut state_writer = state;
        async move {
            // Get the event buffer for storing/replaying events
            let buffer = event_buffer();

            // Create the stream config
            let stream_config = match FILE_PATH.get().cloned().flatten() {
                Some(path) => StreamConfig::file(path),
                None => StreamConfig::stdin(),
            };

            // Start the event stream
            let mut rx = start_stream(stream_config);

            // Phase 1: Read all events from stream into buffer
            while let Some(event) = rx.recv().await {
                // Check for shutdown
                if playback::is_shutdown_requested() {
                    return;
                }

                buffer.lock().unwrap().push(event.clone());

                // Wait while paused, allow step
                loop {
                    // Check for shutdown
                    if playback::is_shutdown_requested() {
                        return;
                    }
                    if !playback::is_paused() {
                        break; // Not paused, proceed
                    }
                    if playback::take_step_request() {
                        break; // Step requested
                    }
                    if playback::take_reset_request() {
                        // Reset during initial load - clear and restart
                        state_writer.write().reset();
                        playback::reset();
                    }
                    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
                }

                state_writer.write().process_event(event);

                // Only delay if not paused
                if !playback::is_paused() {
                    let delay_ms = playback::get_delay_ms();
                    tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
                }
            }

            // Phase 2: Stream ended - replay mode with position tracking
            let mut replay_pos: usize = buffer.lock().unwrap().len(); // Start at end (all events shown)

            loop {
                // Check for shutdown
                if playback::is_shutdown_requested() {
                    return;
                }

                // Wait for user input (reset, step, or play)
                loop {
                    // Check for shutdown
                    if playback::is_shutdown_requested() {
                        return;
                    }

                    if playback::take_reset_request() {
                        // Reset to beginning
                        state_writer.write().reset();
                        playback::reset();
                        replay_pos = 0;
                        break;
                    }

                    let buffer_len = buffer.lock().unwrap().len();
                    if replay_pos < buffer_len {
                        // There are events to process
                        if !playback::is_paused() {
                            break; // Playing - process next event
                        }
                        if playback::take_step_request() {
                            break; // Step requested
                        }
                    }

                    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
                }

                // Process events from current position
                let events: Vec<StreamEvent> = buffer.lock().unwrap().clone();
                while replay_pos < events.len() {
                    // Check for shutdown
                    if playback::is_shutdown_requested() {
                        return;
                    }

                    // Check for reset
                    if playback::take_reset_request() {
                        state_writer.write().reset();
                        playback::reset();
                        replay_pos = 0;
                        break;
                    }

                    // Process event at current position
                    let event = events[replay_pos].clone();
                    state_writer.write().process_event(event);
                    replay_pos += 1;

                    // If paused, wait for next step or unpause
                    if playback::is_paused() {
                        break; // Go back to waiting for input
                    }

                    // Delay between events when playing
                    let delay_ms = playback::get_delay_ms();
                    tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
                }
            }
        }
    });

    rsx! {
        App { state }
    }
}
