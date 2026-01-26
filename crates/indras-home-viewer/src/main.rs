//! Entry point for the home realm viewer.
//!
//! This Dioxus desktop application plays through home realm Lua scenarios
//! from a single user's perspective.

use std::path::PathBuf;
use std::sync::OnceLock;

use clap::Parser;
use dioxus::desktop::{Config, LogicalSize, WindowBuilder};
use dioxus::prelude::*;
use tokio::time::{sleep, Duration};

use indras_home_viewer::components::App;
use indras_home_viewer::events::{start_stream, HomeRealmEvent, StreamConfig};
use indras_home_viewer::playback;
use indras_home_viewer::state::AppState;

/// CSS styles embedded at compile time.
const STYLES_CSS: &str = include_str!("../assets/styles.css");

/// Global storage for the file path argument.
static FILE_PATH: OnceLock<Option<PathBuf>> = OnceLock::new();

/// Global storage for the member filter argument.
static MEMBER_FILTER: OnceLock<Option<String>> = OnceLock::new();

/// Command line arguments.
#[derive(Parser, Debug)]
#[command(name = "indras-home-viewer")]
#[command(about = "First-person home realm viewer for Indras Network")]
struct Args {
    /// Path to a JSONL file to read events from (reads from stdin if not provided)
    #[arg(short, long)]
    file: Option<PathBuf>,

    /// Filter events to show only this member's perspective
    #[arg(short, long)]
    member: Option<String>,

    /// Initial playback speed (default: 1.0)
    #[arg(short, long, default_value = "1.0")]
    speed: f32,

    /// Start playing immediately (default: start paused)
    #[arg(long)]
    autoplay: bool,
}

fn main() {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .init();

    tracing::info!("Starting Home Realm Viewer");

    // Parse command line arguments
    let args = Args::parse();

    // Store args in global state
    FILE_PATH.set(args.file).ok();
    MEMBER_FILTER.set(args.member).ok();

    // Set initial playback state
    playback::set_speed(args.speed);
    playback::set_paused(!args.autoplay); // Start paused unless --autoplay is set

    // Launch the Dioxus desktop app
    dioxus::LaunchBuilder::desktop()
        .with_cfg(
            Config::new()
                .with_window(
                    WindowBuilder::new()
                        .with_title("Home Realm Viewer - Indras Network")
                        .with_inner_size(LogicalSize::new(1400, 900))
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
        .launch(RootApp);
}

/// Root application component that manages the event stream.
#[component]
fn RootApp() -> Element {
    // Create state signal
    let state = use_signal(AppState::new);

    // Request shutdown on unmount
    use_drop(|| {
        tracing::info!("Shutting down Home Realm Viewer");
        playback::request_shutdown();
    });

    // Start the event stream processor
    let _stream_handle = use_resource(move || {
        let mut state = state;

        async move {
            // Get configuration from global state
            let file_path = FILE_PATH.get().and_then(|p| p.clone());
            let member_filter = MEMBER_FILTER.get().and_then(|m| m.clone());

            let config = StreamConfig {
                file_path,
                member_filter: member_filter.clone(),
            };

            tracing::info!("Starting event stream with config: {:?}", config);

            // Get the event buffer for replay
            let buffer = indras_home_viewer::events::event_buffer();

            // Start the stream reader
            let mut rx = start_stream(config);

            // Phase 1: Read events from stream, respecting pause/play
            while let Some(event) = rx.recv().await {
                if playback::is_shutdown_requested() {
                    return;
                }

                // Store in buffer for replay
                buffer.lock().unwrap().push(event.clone());

                // Wait while paused, allow step
                loop {
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
                        state.write().reset();
                    }
                    sleep(Duration::from_millis(50)).await;
                }

                process_event(&mut state, event);

                // Delay between events when playing
                if !playback::is_paused() {
                    let delay = playback::get_delay_ms();
                    if delay > 0 {
                        sleep(Duration::from_millis(delay)).await;
                    }
                }
            }

            // Phase 2: Stream ended - replay mode with position tracking
            let mut replay_pos: usize = buffer.lock().unwrap().len();

            loop {
                if playback::is_shutdown_requested() {
                    return;
                }

                // Wait for user input (reset, step, or play)
                loop {
                    if playback::is_shutdown_requested() {
                        return;
                    }

                    if playback::take_reset_request() {
                        // Reset to beginning
                        state.write().reset();
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

                    sleep(Duration::from_millis(50)).await;
                }

                // Process events from current position
                let events: Vec<HomeRealmEvent> = buffer.lock().unwrap().clone();
                while replay_pos < events.len() {
                    if playback::is_shutdown_requested() {
                        return;
                    }

                    // Check for reset
                    if playback::take_reset_request() {
                        state.write().reset();
                        replay_pos = 0;
                        break;
                    }

                    // Process event at current position
                    let event = events[replay_pos].clone();
                    process_event(&mut state, event);
                    replay_pos += 1;

                    // If paused, wait for next step or unpause
                    if playback::is_paused() {
                        break;
                    }

                    // Delay between events when playing
                    let delay = playback::get_delay_ms();
                    if delay > 0 {
                        sleep(Duration::from_millis(delay)).await;
                    }
                }
            }
        }
    });

    rsx! {
        App { state }
    }
}

/// Processes a single event, updating state.
fn process_event(state: &mut Signal<AppState>, event: HomeRealmEvent) {
    // Log significant events
    match &event {
        HomeRealmEvent::SessionStarted { member, .. } => {
            tracing::info!("Session started for member: {}", member);
        }
        HomeRealmEvent::NoteCreated { title, .. } => {
            tracing::debug!("Note created: {}", title);
        }
        HomeRealmEvent::HomeQuestCreated { title, .. } => {
            tracing::debug!("Quest created: {}", title);
        }
        HomeRealmEvent::ArtifactUploaded { mime_type, size, .. } => {
            tracing::debug!("Artifact uploaded: {} ({} bytes)", mime_type, size);
        }
        _ => {}
    }

    // Update state
    state.write().process_event(event);
}
