use crate::components::*;
use crate::layout::compute_layout;
use crate::runner::document_runner::DocumentRunner;
use crate::runner::{MetricsUpdate, ScenarioRunner};
use crate::state::*;
use crate::theme::{ThemeSwitcher, ThemedRoot};
use dioxus::prelude::*;
use indras_simulation::{from_edges, MeshBuilder, NetworkEvent, PacketId, SimConfig, Simulation};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::Mutex;

/// Root App component for the Indras Network dashboard
///
/// Manages the main application state and layout structure:
/// - Scenario selection and execution
/// - Stress test level configuration
/// - Live metrics monitoring
/// - Event logging and history
#[component]
pub fn App() -> Element {
    // State signals for reactive UI
    let mut selected_scenario = use_signal(|| None::<String>);
    let mut stress_level = use_signal(|| "medium".to_string());
    let mut running = use_signal(|| false);
    let mut metrics = use_signal(SimMetrics::default);
    let mut events = use_signal(Vec::<SimEvent>::new);
    let mut results_history = use_signal(Vec::<TestResult>::new);
    let mut error_message = use_signal(|| None::<String>);

    // Tab navigation state
    let mut current_tab = use_signal(|| Tab::Metrics);

    // Instance view state
    let mut instance_state = use_signal(InstanceState::new);

    // Document view state
    let mut document_state = use_signal(DocumentState::new);
    let mut document_runner: Signal<Option<DocumentRunner>> = use_signal(|| None);

    // SDK view state
    let mut sdk_state = use_signal(SDKState::new);
    let mut sdk_cancel_token: Signal<Option<Arc<Mutex<bool>>>> = use_signal(|| None);

    // Channel for receiving metrics updates from background task
    let mut cancel_token: Signal<Option<Arc<Mutex<bool>>>> = use_signal(|| None);

    // Handle running a scenario
    let mut run_scenario = move |_| {
        if running() {
            return;
        }

        let scenario = match selected_scenario() {
            Some(s) => s,
            None => return,
        };

        let level_str = stress_level();
        let level = match level_str.as_str() {
            "quick" => StressLevel::Quick,
            "full" => StressLevel::Full,
            _ => StressLevel::Medium,
        };

        // Reset state
        running.set(true);
        metrics.set(SimMetrics::default());
        events.set(Vec::new());
        error_message.set(None);

        // Create cancel token
        let token = Arc::new(Mutex::new(false));
        cancel_token.set(Some(token.clone()));

        // Spawn async task to run the scenario
        spawn(async move {
            let runner = ScenarioRunner::new();
            let (tx, mut rx) = mpsc::channel::<MetricsUpdate>(100);

            // Spawn the scenario runner in a separate task
            let scenario_clone = scenario.clone();
            let run_handle =
                tokio::spawn(async move { runner.run_scenario(&scenario_clone, level, tx).await });

            // Process updates as they come in
            while let Some(update) = rx.recv().await {
                // Check if cancelled
                if *token.lock().await {
                    break;
                }

                match update {
                    MetricsUpdate::Stats(new_metrics) => {
                        // Merge metrics to preserve values from earlier updates
                        let mut m = metrics();
                        m.merge(&new_metrics);
                        metrics.set(m);
                    }
                    MetricsUpdate::Event(event) => {
                        events.write().push(event);
                    }
                    MetricsUpdate::Tick { current, max } => {
                        let mut m = metrics();
                        m.current_tick = current;
                        m.max_ticks = max;
                        metrics.set(m);
                    }
                    MetricsUpdate::Complete(result) => {
                        results_history.write().push(result.clone());
                        // Add completion event
                        events.write().push(SimEvent {
                            tick: result.metrics.current_tick,
                            event_type: if result.passed {
                                EventType::Success
                            } else {
                                EventType::Error
                            },
                            description: if result.passed {
                                format!("Scenario {} completed successfully", scenario)
                            } else {
                                format!("Scenario {} failed: {:?}", scenario, result.errors)
                            },
                        });
                    }
                    MetricsUpdate::Error(err) => {
                        error_message.set(Some(err.clone()));
                        events.write().push(SimEvent {
                            tick: 0,
                            event_type: EventType::Error,
                            description: err,
                        });
                    }
                }
            }

            // Wait for the run to complete
            let _ = run_handle.await;
            running.set(false);
            cancel_token.set(None);
        });
    };

    // Handle stopping a scenario
    let stop_scenario = move |_| {
        if let Some(token) = cancel_token() {
            // Set cancel flag
            spawn(async move {
                *token.lock().await = true;
            });
        }
        running.set(false);
        events.write().push(SimEvent {
            tick: metrics().current_tick,
            event_type: EventType::Warning,
            description: "Scenario execution stopped by user".to_string(),
        });
    };

    // Auto-play loop: advance simulation when not paused
    let _playback_loop = use_future(move || async move {
        loop {
            // Check conditions
            let should_step = {
                let state = instance_state.read();
                current_tab() == Tab::Simulations && !state.paused && state.simulation.is_some()
            };

            if should_step {
                let speed = instance_state.read().playback_speed;
                let delay_ms = (1000.0 / speed) as u64;
                tokio::time::sleep(std::time::Duration::from_millis(delay_ms.max(50))).await;

                // Step the simulation and collect events
                let new_events = {
                    let mut state_write = instance_state.write();
                    if let Some(ref mut sim) = state_write.simulation {
                        // Check if simulation should continue
                        if sim.tick >= sim.config.max_ticks {
                            state_write.paused = true;
                            continue;
                        }
                        sim.step();
                        sim.event_log.clone()
                    } else {
                        continue;
                    }
                };

                // Update events and create animations in a separate borrow
                let mut state_write = instance_state.write();
                let current_count = state_write.recent_events.len();
                let current_tick = state_write.current_tick();

                for event in new_events.into_iter().skip(current_count) {
                    // Create packet animations for visual movement
                    match &event {
                        NetworkEvent::Send { from, to, .. } => {
                            // Direct send - animate from sender to receiver
                            let packet_id = PacketId {
                                source: *from,
                                sequence: current_tick,
                            };
                            state_write.packets_in_flight.push(PacketAnimation::new(
                                packet_id,
                                *from,
                                *to,
                                current_tick,
                            ));
                        }
                        NetworkEvent::Relay {
                            from: _,
                            via,
                            to,
                            packet_id,
                            ..
                        } => {
                            // Relay - animate from via peer toward destination
                            state_write.packets_in_flight.push(PacketAnimation::new(
                                *packet_id,
                                *via,
                                *to,
                                current_tick,
                            ));
                        }
                        NetworkEvent::Delivered {
                            packet_id, to: _, ..
                        } => {
                            // Remove completed animations for this packet
                            state_write
                                .packets_in_flight
                                .retain(|p| p.packet_id != *packet_id);
                        }
                        _ => {}
                    }
                    state_write.add_event(event);
                }

                // Update animation progress and remove completed ones
                state_write
                    .packets_in_flight
                    .iter_mut()
                    .for_each(|p| p.update(current_tick));
                state_write.packets_in_flight.retain(|p| !p.is_complete());
            } else {
                // When paused or not on instances tab, poll less frequently
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        }
    });

    // Auto-play loop for Documents tab
    let _document_playback_loop = use_future(move || async move {
        loop {
            // Check conditions for document auto-play
            let should_step = {
                let state = document_state.read();
                current_tab() == Tab::Documents && state.running && state.scenario_name.is_some()
            };

            if should_step {
                // Slower step rate for documents (500ms between steps)
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;

                // Execute a step
                let update = {
                    if let Some(ref mut runner) = *document_runner.write() {
                        runner.step()
                    } else {
                        None
                    }
                };

                if let Some(update) = update {
                    let mut state = document_state.write();
                    match update {
                        crate::runner::document_runner::DocumentUpdate::Event(event) => {
                            state.add_event(event);
                        }
                        crate::runner::document_runner::DocumentUpdate::Complete { .. } => {
                            state.running = false;
                        }
                        _ => {}
                    }

                    // Update peer states from runner
                    if let Some(ref runner) = *document_runner.read() {
                        state.peers = runner.get_peer_states();
                        state.current_step = runner.current_step;
                        state.is_converged = runner.check_convergence();
                    }
                }
            } else {
                // When not running or not on documents tab, poll less frequently
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        }
    });

    rsx! {
        ThemedRoot {
            ThemeSwitcher {}
            div { class: "dashboard",
                // Header section
                Header {}

            // Tab bar at top level
            div { class: "tab-bar-container", style: "background: var(--bg-secondary); padding: 0 var(--spacing-lg); border-bottom: 1px solid var(--border-color);",
                TabBar {
                    current_tab: current_tab(),
                    on_select: move |tab: Tab| {
                        current_tab.set(tab);
                    }
                }
            }

            // Main content layout - different for each tab
            match current_tab() {
                Tab::Metrics => rsx! {
                    div { class: "main-content",
                        // Left sidebar for scenario selection (only in Metrics tab)
                        Sidebar {
                            selected: selected_scenario(),
                            on_select: move |scenario: String| {
                                if !running() {
                                    selected_scenario.set(Some(scenario));
                                }
                            }
                        }

                        // Right content area
                        div { class: "content",
                            // Error banner if present
                            if let Some(err) = error_message() {
                                div {
                                    class: "error-banner",
                                    style: "background: #ef4444; color: white; padding: 8px 16px; border-radius: 4px; margin-bottom: 16px;",
                                    "Error: {err}"
                                }
                            }

                            // Scenario description panel
                            ScenarioDescription {
                                selected: selected_scenario()
                            }

                            // Control panel for running tests
                            Controls {
                                selected: selected_scenario(),
                                level: stress_level(),
                                running: running(),
                                on_run: run_scenario,
                                on_stop: stop_scenario,
                                on_level_change: move |level: String| {
                                    if !running() {
                                        stress_level.set(level);
                                    }
                                }
                            }

                            // Live metrics display
                            MetricsPanel {
                                metrics: metrics()
                            }

                            // Event log with recent events
                            EventLog {
                                events: events()
                            }
                        }
                    }
                },
                Tab::Simulations => rsx! {
                    // Simulations view with sidebar for scenario selection
                    SimulationsView {
                        state: instance_state,
                        on_load_scenario: move |scenario: String| {
                            // Create mesh based on scenario type
                            let mesh = match scenario.as_str() {
                                "triangle" => from_edges(&[('A', 'B'), ('B', 'C'), ('A', 'C')]),
                                "line" => MeshBuilder::new(5).line(),
                                "star" => MeshBuilder::new(6).star(),
                                "ring" => MeshBuilder::new(8).ring(),
                                "mesh" => MeshBuilder::new(5).full_mesh(),
                                _ => from_edges(&[('A', 'B'), ('B', 'C'), ('A', 'C')]),
                            };

                            let config = SimConfig {
                                max_ticks: 200,
                                wake_probability: 0.2,
                                sleep_probability: 0.05,
                                initial_online_probability: 0.0,
                                ..Default::default()
                            };
                            let mut sim = Simulation::new(mesh.clone(), config);
                            sim.initialize();

                            // Bring some peers online based on topology
                            let peer_ids = sim.mesh.peer_ids();
                            let online_count = (peer_ids.len() / 2).max(2);
                            for peer in peer_ids.iter().take(online_count) {
                                sim.force_online(*peer);
                            }

                            // Queue messages between various peers
                            if peer_ids.len() >= 3 {
                                // Send from first online peer to last peer (likely offline)
                                let sender = peer_ids[0];
                                let receiver = peer_ids[peer_ids.len() - 1];
                                sim.send_message(sender, receiver, b"Hello!".to_vec());

                                // Send between online peers
                                if peer_ids.len() >= 2 {
                                    sim.send_message(peer_ids[1], peer_ids[0], b"Hi back!".to_vec());
                                }
                            }

                            let positions = compute_layout(&mesh, 700.0, 400.0);

                            // Clear and set up state
                            let mut state = instance_state.write();
                            state.simulation = Some(sim);
                            state.peer_positions = positions;
                            state.scenario_name = Some(scenario);
                            state.paused = true;
                            state.recent_events.clear();
                            state.packets_in_flight.clear();
                        }
                    }
                },
                Tab::Documents => rsx! {
                    // Full-width content for Documents tab
                    div { class: "content", style: "padding: var(--spacing-lg);",
                        DocumentsView {
                            state: document_state,
                            on_load_scenario: move |scenario: String| {
                                // Load the selected document scenario
                                let mut runner = DocumentRunner::new();
                                if let Ok(total_steps) = runner.load_scenario(&scenario) {
                                    // Reset document state
                                    let mut state = document_state.write();
                                    state.reset();
                                    state.scenario_name = Some(scenario);
                                    state.total_steps = total_steps;

                                    // Store the runner
                                    document_runner.set(Some(runner));
                                }
                            },
                            on_run: move |_| {
                                // Start auto-play mode
                                document_state.write().running = true;
                            },
                            on_pause: move |_| {
                                // Pause auto-play
                                document_state.write().running = false;
                            },
                            on_reset: move |_| {
                                // Reset to beginning of scenario
                                let scenario_name = document_state.read().scenario_name.clone();
                                if let Some(scenario_name) = scenario_name {
                                    let mut runner = DocumentRunner::new();
                                    if let Ok(total_steps) = runner.load_scenario(&scenario_name) {
                                        let mut state = document_state.write();
                                        state.reset();
                                        state.scenario_name = Some(scenario_name);
                                        state.total_steps = total_steps;
                                        document_runner.set(Some(runner));
                                    }
                                }
                            },
                            on_step: move |_| {
                                // Execute single step
                                if let Some(ref mut runner) = *document_runner.write() {
                                    if let Some(update) = runner.step() {
                                        let mut state = document_state.write();
                                        match update {
                                            crate::runner::document_runner::DocumentUpdate::Event(event) => {
                                                state.add_event(event);
                                            }
                                            crate::runner::document_runner::DocumentUpdate::Complete { .. } => {
                                                state.running = false;
                                            }
                                            _ => {}
                                        }
                                        // Update peer states
                                        state.peers = runner.get_peer_states();
                                        state.current_step = runner.current_step;
                                        state.is_converged = runner.check_convergence();
                                    }
                                }
                            },
                        }
                    }
                },
                Tab::SDK => rsx! {
                    // Full-width content for SDK tab
                    div { class: "content", style: "padding: 0;",
                        SDKView {
                            state: sdk_state,
                            on_run: move |_| {
                                let current_dashboard = sdk_state.read().current_dashboard;
                                let scenario = current_dashboard.scenario_name().to_string();
                                let level_str = sdk_state.read().stress_level.clone();
                                let level = match level_str.as_str() {
                                    "quick" => StressLevel::Quick,
                                    "full" => StressLevel::Full,
                                    _ => StressLevel::Medium,
                                };

                                // Reset state
                                sdk_state.write().reset();
                                sdk_state.write().running = true;

                                // Create cancel token
                                let token = Arc::new(Mutex::new(false));
                                sdk_cancel_token.set(Some(token.clone()));

                                // Spawn async task to run the scenario
                                spawn(async move {
                                    let runner = ScenarioRunner::new();
                                    let (tx, mut rx) = mpsc::channel::<MetricsUpdate>(100);

                                    // Spawn the scenario runner
                                    let scenario_clone = scenario.clone();
                                    let run_handle = tokio::spawn(async move {
                                        runner.run_scenario(&scenario_clone, level, tx).await
                                    });

                                    // Process updates
                                    while let Some(update) = rx.recv().await {
                                        if *token.lock().await {
                                            break;
                                        }

                                        match update {
                                            MetricsUpdate::Stats(new_metrics) => {
                                                // Merge metrics to preserve values from earlier updates
                                                sdk_state.write().metrics.merge(&new_metrics);
                                            }
                                            MetricsUpdate::Event(event) => {
                                                sdk_state.write().add_event(event);
                                            }
                                            MetricsUpdate::Tick { current, max } => {
                                                let mut state = sdk_state.write();
                                                state.metrics.current_tick = current;
                                                state.metrics.max_ticks = max;
                                            }
                                            MetricsUpdate::Complete(result) => {
                                                sdk_state.write().add_event(SimEvent {
                                                    tick: result.metrics.current_tick,
                                                    event_type: if result.passed {
                                                        EventType::Success
                                                    } else {
                                                        EventType::Error
                                                    },
                                                    description: if result.passed {
                                                        format!("{} completed successfully", scenario)
                                                    } else {
                                                        format!("{} failed: {:?}", scenario, result.errors)
                                                    },
                                                });
                                            }
                                            MetricsUpdate::Error(err) => {
                                                sdk_state.write().add_event(SimEvent {
                                                    tick: 0,
                                                    event_type: EventType::Error,
                                                    description: err,
                                                });
                                            }
                                        }
                                    }

                                    let _ = run_handle.await;
                                    sdk_state.write().running = false;
                                    sdk_cancel_token.set(None);
                                });
                            },
                            on_stop: move |_| {
                                if let Some(token) = sdk_cancel_token() {
                                    spawn(async move {
                                        *token.lock().await = true;
                                    });
                                }
                                sdk_state.write().running = false;
                                let current_tick = sdk_state.read().metrics.current_tick;
                                sdk_state.write().add_event(SimEvent {
                                    tick: current_tick,
                                    event_type: EventType::Warning,
                                    description: "Test execution stopped by user".to_string(),
                                });
                            },
                            on_level_change: move |level: String| {
                                if !sdk_state.read().running {
                                    sdk_state.write().stress_level = level;
                                }
                            },
                        }
                    }
                },
            }
        }

        // Unified Control Bar at the bottom (outside dashboard, inside ThemedRoot)
        UnifiedControlBar {
                playback_state: UnifiedPlaybackState::from_tab(
                    current_tab(),
                    &instance_state.read(),
                    &document_state.read(),
                    &sdk_state.read(),
                    &metrics(),
                    running(),
                    selected_scenario().as_deref(),
                    &stress_level(),
                ),
                on_step: move |_| {
                    match current_tab() {
                        Tab::Simulations => {
                            // Step the simulation
                            let new_events = {
                                let mut state_write = instance_state.write();
                                if let Some(ref mut sim) = state_write.simulation {
                                    sim.step();
                                    sim.event_log.clone()
                                } else {
                                    return;
                                }
                            };
                            let mut state_write = instance_state.write();
                            let current_count = state_write.recent_events.len();
                            let current_tick = state_write.current_tick();

                            for event in new_events.into_iter().skip(current_count) {
                                match &event {
                                    NetworkEvent::Send { from, to, .. } => {
                                        let packet_id = PacketId { source: *from, sequence: current_tick };
                                        state_write.packets_in_flight.push(PacketAnimation::new(
                                            packet_id, *from, *to, current_tick
                                        ));
                                    }
                                    NetworkEvent::Relay { via, to, packet_id, .. } => {
                                        state_write.packets_in_flight.push(PacketAnimation::new(
                                            *packet_id, *via, *to, current_tick
                                        ));
                                    }
                                    NetworkEvent::Delivered { packet_id, .. } => {
                                        state_write.packets_in_flight.retain(|p| p.packet_id != *packet_id);
                                    }
                                    _ => {}
                                }
                                state_write.add_event(event);
                            }
                            state_write.packets_in_flight.iter_mut().for_each(|p| p.update(current_tick));
                            state_write.packets_in_flight.retain(|p| !p.is_complete());
                        }
                        Tab::Documents => {
                            // Step the document scenario
                            if let Some(ref mut runner) = *document_runner.write() {
                                let mut state = document_state.write();
                                if state.current_step < state.total_steps {
                                    if let Some(update) = runner.step() {
                                        use crate::runner::document_runner::DocumentUpdate;
                                        match update {
                                            DocumentUpdate::Event(evt) => {
                                                state.add_event(evt);
                                            }
                                            DocumentUpdate::PeerState { peer_name, notebook_name, notes, heads } => {
                                                use crate::state::document::{PeerDocumentState, NoteSnapshot};
                                                let peer_state = PeerDocumentState {
                                                    peer_name: peer_name.clone(),
                                                    notebook_name,
                                                    notes: notes.iter().map(|n| NoteSnapshot {
                                                        id: n.id.clone(),
                                                        title: n.title.clone(),
                                                        content_preview: n.content_preview.clone(),
                                                        author: n.author.clone(),
                                                    }).collect(),
                                                    heads,
                                                    note_count: notes.len(),
                                                };
                                                state.peers.insert(peer_name, peer_state);
                                            }
                                            DocumentUpdate::Complete { .. } => {
                                                state.running = false;
                                            }
                                            DocumentUpdate::StepComplete { .. } => {
                                                // Just continue
                                            }
                                            DocumentUpdate::ScenarioLoaded { .. } => {
                                                // Already handled
                                            }
                                            DocumentUpdate::Error(_) => {
                                                state.running = false;
                                            }
                                        }
                                        state.current_step += 1;
                                    }
                                }
                            }
                        }
                        _ => {} // No step for SDK/Metrics
                    }
                },
                on_play_pause: move |_| {
                    match current_tab() {
                        Tab::Simulations => {
                            let paused = instance_state.read().paused;
                            instance_state.write().paused = !paused;
                        }
                        Tab::Documents => {
                            let is_running = document_state.read().running;
                            document_state.write().running = !is_running;
                        }
                        Tab::SDK => {
                            let is_running = sdk_state.read().running;
                            if is_running {
                                // Stop
                                if let Some(token) = sdk_cancel_token() {
                                    spawn(async move {
                                        *token.lock().await = true;
                                    });
                                }
                                sdk_state.write().running = false;
                            } else {
                                // Start SDK test
                                let current_dashboard = sdk_state.read().current_dashboard;
                                let scenario = current_dashboard.scenario_name().to_string();
                                let level_str = sdk_state.read().stress_level.clone();
                                let level = match level_str.as_str() {
                                    "quick" => StressLevel::Quick,
                                    "full" => StressLevel::Full,
                                    _ => StressLevel::Medium,
                                };

                                sdk_state.write().reset();
                                sdk_state.write().running = true;

                                let token = Arc::new(Mutex::new(false));
                                sdk_cancel_token.set(Some(token.clone()));

                                spawn(async move {
                                    let runner = ScenarioRunner::new();
                                    let (tx, mut rx) = mpsc::channel::<MetricsUpdate>(100);

                                    let scenario_clone = scenario.clone();
                                    let run_handle = tokio::spawn(async move {
                                        runner.run_scenario(&scenario_clone, level, tx).await
                                    });

                                    while let Some(update) = rx.recv().await {
                                        if *token.lock().await {
                                            break;
                                        }
                                        match update {
                                            MetricsUpdate::Stats(new_metrics) => {
                                                sdk_state.write().metrics.merge(&new_metrics);
                                            }
                                            MetricsUpdate::Event(event) => {
                                                sdk_state.write().add_event(event);
                                            }
                                            MetricsUpdate::Tick { current, max } => {
                                                let mut state = sdk_state.write();
                                                state.metrics.current_tick = current;
                                                state.metrics.max_ticks = max;
                                            }
                                            MetricsUpdate::Complete(result) => {
                                                sdk_state.write().add_event(SimEvent {
                                                    tick: result.metrics.current_tick,
                                                    event_type: if result.passed { EventType::Success } else { EventType::Error },
                                                    description: if result.passed {
                                                        format!("{} completed successfully", scenario)
                                                    } else {
                                                        format!("{} failed: {:?}", scenario, result.errors)
                                                    },
                                                });
                                            }
                                            MetricsUpdate::Error(err) => {
                                                sdk_state.write().add_event(SimEvent {
                                                    tick: 0,
                                                    event_type: EventType::Error,
                                                    description: err,
                                                });
                                            }
                                        }
                                    }

                                    let _ = run_handle.await;
                                    sdk_state.write().running = false;
                                    sdk_cancel_token.set(None);
                                });
                            }
                        }
                        Tab::Metrics => {
                            let is_running = running();
                            if is_running {
                                // Stop
                                if let Some(token) = cancel_token() {
                                    spawn(async move {
                                        *token.lock().await = true;
                                    });
                                }
                                running.set(false);
                            } else {
                                // Start - call the run_scenario logic
                                run_scenario(());
                            }
                        }
                    }
                },
                on_reset: move |_| {
                    match current_tab() {
                        Tab::Simulations => {
                            instance_state.write().simulation = None;
                            instance_state.write().clear_events();
                            instance_state.write().packets_in_flight.clear();
                            instance_state.write().peer_positions.clear();
                            instance_state.write().scenario_name = None;
                        }
                        Tab::Documents => {
                            document_state.write().reset();
                            document_runner.set(None);
                        }
                        Tab::SDK => {
                            if let Some(token) = sdk_cancel_token() {
                                spawn(async move {
                                    *token.lock().await = true;
                                });
                            }
                            sdk_state.write().reset();
                        }
                        Tab::Metrics => {
                            if let Some(token) = cancel_token() {
                                spawn(async move {
                                    *token.lock().await = true;
                                });
                            }
                            running.set(false);
                            metrics.set(SimMetrics::default());
                            events.set(Vec::new());
                        }
                    }
                },
                on_speed_change: move |speed: f64| {
                    if current_tab() == Tab::Simulations {
                        instance_state.write().playback_speed = speed;
                    }
                },
                on_level_change: move |level: String| {
                    match current_tab() {
                        Tab::Metrics => {
                            if !running() {
                                stress_level.set(level);
                            }
                        }
                        Tab::SDK => {
                            if !sdk_state.read().running {
                                sdk_state.write().stress_level = level;
                            }
                        }
                        _ => {}
                    }
                },
            }
        }
    }
}
