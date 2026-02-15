//! Omni Viewer - Calm Observation Dashboard
//!
//! Per-member "pulse" layout. Each column shows only what's unique to that member.
//!
//! Usage:
//!   lua_runner scenario.lua | omni-viewer
//!   omni-viewer --file events.jsonl
//!   omni-viewer                           (TTY: shows scenario picker)

use std::path::PathBuf;
use std::sync::OnceLock;

use clap::Parser;
use dioxus::prelude::*;

use indras_realm_viewer::components::omni::OmniApp;
use indras_realm_viewer::components::scenario_picker::ScenarioPicker;
use indras_realm_viewer::events::{start_stream, StreamConfig, StreamEvent};
use indras_realm_viewer::playback;
use indras_realm_viewer::state::{clear_event_buffer, event_buffer, AppState};
use indras_realm_viewer::theme::ThemedRoot;

/// Embedded CSS styles
const SHARED_CSS: &str = indras_ui::SHARED_CSS;
const STYLES_CSS: &str = include_str!("../assets/styles.css");
const OMNI_STYLES_CSS: &str = include_str!("../assets/omni_styles.css");

/// Global file path for stream config
static FILE_PATH: OnceLock<Option<PathBuf>> = OnceLock::new();

/// Whether stdin is a TTY (no piped input)
static IS_TTY: OnceLock<bool> = OnceLock::new();

/// Command-line arguments
#[derive(Parser, Debug)]
#[command(name = "omni-viewer")]
#[command(about = "Calm observation dashboard showing per-member pulse views")]
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

    // Detect TTY before clap potentially touches stdin
    IS_TTY
        .set(std::io::IsTerminal::is_terminal(&std::io::stdin()))
        .ok();

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
                        .with_title("Omni Viewer - Indras Network")
                        .with_inner_size(dioxus::desktop::LogicalSize::new(1920, 1080))
                        .with_resizable(true)
                        .with_maximized(true),
                )
                .with_custom_head(format!(
                    r#"<style>{}</style><style>{}</style><style>{}</style>"#,
                    SHARED_CSS, STYLES_CSS, OMNI_STYLES_CSS
                )),
        )
        .launch(RootApp);
}

/// Application mode
#[derive(Clone, Copy, Debug, PartialEq)]
enum AppMode {
    Picker,
    Streaming,
}

/// Root application component
fn RootApp() -> Element {
    let is_tty = IS_TTY.get().copied().unwrap_or(false);
    let has_file = FILE_PATH
        .get()
        .map(|f| f.is_some())
        .unwrap_or(false);

    let initial_mode = if is_tty && !has_file {
        AppMode::Picker
    } else {
        AppMode::Streaming
    };

    let mut app_mode = use_signal(|| initial_mode);
    let mut selected_scenario = use_signal(|| None::<PathBuf>);

    // Create app state signal
    let mut state = use_signal(AppState::new);

    // Request shutdown when component unmounts (window closing)
    use_drop(|| {
        playback::request_shutdown();
    });

    // Stream resource - always registered but only runs when Streaming
    let _stream_handle = use_resource(move || {
        let mut state_writer = state;
        async move {
            // Wait until we're in streaming mode
            if *app_mode.read() != AppMode::Streaming {
                // Return early; the resource will re-run when app_mode changes
                // because we read it reactively above
                return;
            }

            // Get the event buffer for storing/replaying events
            let buffer = event_buffer();

            // Build stream config based on source
            let stream_config = if let Some(scenario_path) = selected_scenario.read().clone() {
                // Subprocess mode: spawn lua_runner
                let manifest_path = PathBuf::from("simulation/Cargo.toml");
                StreamConfig::subprocess(scenario_path, manifest_path)
            } else if let Some(file_path) = FILE_PATH.get().cloned().flatten() {
                StreamConfig::file(file_path)
            } else {
                StreamConfig::stdin()
            };

            // Start the event stream
            let mut rx = start_stream(stream_config);

            // Phase 1: Read all events from stream into buffer
            'live: while let Some(event) = rx.recv().await {
                // Check for shutdown
                if playback::is_shutdown_requested() {
                    return;
                }

                buffer.lock().unwrap().push(event.clone());
                let buf_len = buffer.lock().unwrap().len();
                playback::set_buffer_len(buf_len);

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
                    // Handle seek while paused in Phase 1
                    if let Some(target) = playback::take_seek_request() {
                        let events: Vec<_> = buffer.lock().unwrap().clone();
                        let clamped = target.min(events.len());
                        state_writer.write().reset();
                        for ev in &events[..clamped] {
                            state_writer.write().process_event(ev.clone());
                        }
                        playback::set_current_pos(clamped);
                        playback::set_paused(true);
                        state_writer.write().playback.paused = true;
                        continue 'live;
                    }
                    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
                }

                state_writer.write().process_event(event);
                playback::set_current_pos(buf_len);

                // Only delay if not paused
                if !playback::is_paused() {
                    let delay_ms = playback::get_delay_ms();
                    tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
                }
            }

            // Phase 2: Stream ended - replay mode with position tracking
            let mut replay_pos: usize = buffer.lock().unwrap().len(); // Start at end (all events shown)
            playback::set_current_pos(replay_pos);

            loop {
                // Check for shutdown
                if playback::is_shutdown_requested() {
                    return;
                }

                // Wait for user input (reset, step, play, or seek)
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

                    // Handle seek request
                    if let Some(target) = playback::take_seek_request() {
                        let events: Vec<StreamEvent> = buffer.lock().unwrap().clone();
                        let clamped = target.min(events.len());
                        if clamped < replay_pos {
                            // Backward seek: reset and replay from 0
                            state_writer.write().reset();
                            for ev in &events[..clamped] {
                                state_writer.write().process_event(ev.clone());
                            }
                        } else {
                            // Forward seek: process from current to target
                            for ev in &events[replay_pos..clamped] {
                                state_writer.write().process_event(ev.clone());
                            }
                        }
                        replay_pos = clamped;
                        playback::set_current_pos(replay_pos);
                        playback::set_paused(true);
                        state_writer.write().playback.paused = true;
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

                    // Handle seek during replay
                    if let Some(target) = playback::take_seek_request() {
                        let clamped = target.min(events.len());
                        if clamped < replay_pos {
                            state_writer.write().reset();
                            for ev in &events[..clamped] {
                                state_writer.write().process_event(ev.clone());
                            }
                        } else {
                            for ev in &events[replay_pos..clamped] {
                                state_writer.write().process_event(ev.clone());
                            }
                        }
                        replay_pos = clamped;
                        playback::set_current_pos(replay_pos);
                        playback::set_paused(true);
                        state_writer.write().playback.paused = true;
                        break;
                    }

                    // Process event at current position
                    let event = events[replay_pos].clone();
                    state_writer.write().process_event(event);
                    replay_pos += 1;
                    playback::set_current_pos(replay_pos);

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

    let on_scenario_select = move |path: PathBuf| {
        // Reset all state for fresh scenario
        clear_event_buffer();
        state.write().reset();
        playback::reset();
        playback::set_buffer_len(0);
        playback::set_current_pos(0);

        // Store the selected scenario and switch to streaming
        selected_scenario.set(Some(path));
        app_mode.set(AppMode::Streaming);
    };

    let current_mode = *app_mode.read();
    match current_mode {
        AppMode::Picker => {
            let scenarios_dir = PathBuf::from("simulation/scripts/scenarios");
            rsx! {
                ThemedRoot {
                    ScenarioPicker {
                        on_select: on_scenario_select,
                        scenarios_dir,
                    }
                }
            }
        }
        AppMode::Streaming => {
            rsx! {
                OmniApp { state }
            }
        }
    }
}
