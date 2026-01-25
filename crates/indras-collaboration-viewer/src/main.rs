// Collaboration Viewer - Standalone Dioxus App
//
// A responsive UI that tracks through the collaboration_trio simulation
// with realistic interfaces for quests, documents, and peer interactions.

use std::time::Duration;

use dioxus::prelude::*;

mod components;
mod state;
mod theme;

use components::{ControlBar, Header, PeerPanel, RightPanel, VisualizationPanel};
use state::{CollaborationState, EventType, Peer, Phase, PlanSection, Quest, QuestStatus, ScenarioData};
use theme::{ThemedRoot, ThemeSwitcher};

// Embed CSS
const STYLES_CSS: &str = include_str!("../assets/styles.css");

fn main() {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    // Launch Dioxus desktop app
    dioxus::LaunchBuilder::desktop()
        .with_cfg(
            dioxus::desktop::Config::new()
                .with_window(
                    dioxus::desktop::WindowBuilder::new()
                        .with_title("Collaboration Trio - Indras Network")
                        .with_inner_size(dioxus::desktop::LogicalSize::new(1400, 900))
                        .with_resizable(true),
                )
                .with_custom_head(format!(r#"<style>{}</style>"#, STYLES_CSS)),
        )
        .launch(App);
}

/// Main application component
#[component]
fn App() -> Element {
    // Main simulation state
    let mut state = use_signal(CollaborationState::default);

    // Auto-play loop
    let state_for_future = state.clone();
    use_future(move || {
        let mut state = state_for_future.clone();
        async move {
            let mut accum: f64 = 0.0;
            loop {
                tokio::time::sleep(Duration::from_millis(50)).await;

                let (paused, tick, max_tick, speed) = {
                    let current = state.read();
                    (current.paused, current.tick, current.max_tick, current.speed)
                };

                if !paused && tick < max_tick {
                    accum += 0.05 * speed;
                    if accum >= 0.5 {
                        accum -= 0.5;
                        step_simulation(&mut state);
                    }

                    // Update animations
                    state.write().update_animations(0.05);
                }
            }
        }
    });

    // Event handlers
    let on_step = move |_| {
        step_simulation(&mut state);
    };

    let on_play_pause = move |_| {
        let current_paused = state.read().paused;
        state.write().paused = !current_paused;
    };

    let on_reset = move |_| {
        state.write().reset();
    };

    let on_speed_change = move |speed: f64| {
        state.write().speed = speed;
    };

    let current_state = state.read().clone();

    rsx! {
        ThemedRoot {
            ThemeSwitcher {}
            div { class: "app-container",
                Header { state: current_state.clone() }
                main { class: "main-content",
                    PeerPanel { state: current_state.clone() }
                    VisualizationPanel { state: current_state.clone() }
                    RightPanel { state: current_state.clone() }
                }
                ControlBar {
                    state: current_state,
                    on_step: on_step,
                    on_play_pause: on_play_pause,
                    on_reset: on_reset,
                    on_speed_change: on_speed_change,
                }
            }
        }
    }
}

/// Step the simulation forward by one tick
fn step_simulation(state: &mut Signal<CollaborationState>) {
    let tick = {
        let mut s = state.write();
        s.tick += 1;
        // Clear highlight after a tick
        if s.highlighted_quest.is_some() {
            s.highlighted_quest = None;
        }
        s.tick
    };

    match tick {
        // Phase 1: Setup (ticks 1-5)
        1 => {
            let mut s = state.write();
            s.phase = Phase::Setup;
            s.add_event(EventType::Setup, "Initializing simulation...".into(), None);
        }
        2 => {
            let mut s = state.write();
            if let Some(ps) = s.peer_states.get_mut(&Peer::Love) {
                ps.online = true;
            }
            s.add_event(EventType::Setup, "Love joined the realm".into(), Some(Peer::Love));
        }
        3 => {
            let mut s = state.write();
            if let Some(ps) = s.peer_states.get_mut(&Peer::Joy) {
                ps.online = true;
            }
            s.add_event(EventType::Setup, "Joy joined the realm".into(), Some(Peer::Joy));
        }
        4 => {
            let mut s = state.write();
            if let Some(ps) = s.peer_states.get_mut(&Peer::Peace) {
                ps.online = true;
            }
            s.add_event(EventType::Setup, "Peace joined the realm".into(), Some(Peer::Peace));
        }
        5 => {
            state.write().add_event(EventType::PhaseComplete, "All peers connected".into(), None);
        }

        // Phase 2: Quest Creation (ticks 6-17)
        6 => {
            let mut s = state.write();
            s.phase = Phase::QuestCreation;
            s.add_event(EventType::PhaseComplete, "Starting quest creation phase".into(), None);
        }
        7..=12 => {
            let quest_idx = (tick - 7) as usize;
            let quests = ScenarioData::quests();
            if quest_idx < quests.len() {
                let (creator, title, assignee) = quests[quest_idx];
                let quest_id = (quest_idx + 1) as u32;

                let mut s = state.write();
                // Create quest
                s.quests.push(Quest {
                    id: quest_id,
                    title: title.to_string(),
                    creator,
                    assignee,
                    status: QuestStatus::Pending,
                });

                // Update peer state
                if let Some(ps) = s.peer_states.get_mut(&creator) {
                    ps.quests_created += 1;
                }

                s.highlighted_quest = Some(quest_id);
                s.add_event(
                    EventType::QuestCreated,
                    format!("{} created quest: {}", creator.display_name(), title),
                    Some(creator),
                );

                // Send sync packets to other peers
                for peer in Peer::all() {
                    if *peer != creator {
                        s.send_packet(creator, *peer, "quest_sync");
                        if let Some(ps) = s.peer_states.get_mut(&creator) {
                            ps.messages_sent += 1;
                        }
                    }
                }
            }
        }
        13..=17 => {
            // Sync ticks
            if tick == 13 {
                state.write().add_event(EventType::Sync, "Synchronizing quest data...".into(), None);
            }
        }

        // Phase 3: Document Collaboration (ticks 18-32)
        18 => {
            let mut s = state.write();
            s.phase = Phase::DocumentCollaboration;
            s.add_event(EventType::PhaseComplete, "Starting document collaboration".into(), None);
        }
        20 | 24 | 28 => {
            let section_idx = match tick {
                20 => 0,
                24 => 1,
                28 => 2,
                _ => return,
            };
            let sections = ScenarioData::plan_sections();
            if section_idx < sections.len() {
                let (author, content) = sections[section_idx];

                let mut s = state.write();
                // Add section
                s.plan_sections.push(PlanSection {
                    id: (section_idx + 1) as u32,
                    author,
                    content: content.to_string(),
                });

                // Update peer state
                if let Some(ps) = s.peer_states.get_mut(&author) {
                    ps.sections_written += 1;
                }

                s.add_event(
                    EventType::DocumentSection,
                    format!("{} added a section to the project plan", author.display_name()),
                    Some(author),
                );

                // Send sync packets
                for peer in Peer::all() {
                    if *peer != author {
                        s.send_packet(author, *peer, "doc_sync");
                        if let Some(ps) = s.peer_states.get_mut(&author) {
                            ps.messages_sent += 1;
                        }
                    }
                }
            }
        }
        21..=23 | 25..=27 | 29..=32 => {
            // Sync ticks between sections
        }

        // Phase 4: Quest Updates (ticks 33-42)
        33 => {
            let mut s = state.write();
            s.phase = Phase::QuestUpdates;
            s.add_event(EventType::PhaseComplete, "Starting quest updates".into(), None);
        }
        35 => {
            update_quest_status(state, 1, QuestStatus::InProgress, "started");
        }
        36 => {
            update_quest_status(state, 3, QuestStatus::InProgress, "started");
        }
        37 => {
            update_quest_status(state, 5, QuestStatus::InProgress, "started");
        }
        39 => {
            update_quest_status(state, 1, QuestStatus::Completed, "completed");
        }
        41 => {
            update_quest_status(state, 5, QuestStatus::Completed, "completed");
        }

        // Phase 5: Verification (ticks 43-50)
        43 => {
            let mut s = state.write();
            s.phase = Phase::Verification;
            s.add_event(EventType::PhaseComplete, "Verifying document convergence".into(), None);
        }
        45 => {
            state.write().add_event(EventType::Sync, "All 6 quests synchronized".into(), None);
        }
        47 => {
            state.write().add_event(EventType::Sync, "Project plan converged across all peers".into(), None);
        }
        50 => {
            let mut s = state.write();
            s.phase = Phase::Complete;
            s.add_event(
                EventType::PhaseComplete,
                "Collaboration complete! 100% convergence achieved.".into(),
                None,
            );
        }

        _ => {}
    }
}

/// Helper function to update quest status and log event
fn update_quest_status(state: &mut Signal<CollaborationState>, quest_id: u32, new_status: QuestStatus, action: &str) {
    // First, get quest info
    let quest_info = {
        let s = state.read();
        s.quests.iter().find(|q| q.id == quest_id).map(|q| {
            (q.title.clone(), q.assignee)
        })
    };

    if let Some((title, assignee)) = quest_info {
        let mut s = state.write();

        // Update quest status
        if let Some(quest) = s.quests.iter_mut().find(|q| q.id == quest_id) {
            quest.status = new_status;
        }

        s.highlighted_quest = Some(quest_id);
        s.add_event(
            EventType::QuestUpdated,
            format!("{} {} quest: {}", assignee.display_name(), action, title),
            Some(assignee),
        );

        // Send sync packets for completed quests
        if new_status == QuestStatus::Completed {
            for peer in Peer::all() {
                if *peer != assignee {
                    s.send_packet(assignee, *peer, "status_sync");
                }
            }
        }
    }
}
