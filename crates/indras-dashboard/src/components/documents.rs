//! UI components for the Documents tab
//!
//! Provides visualization of Automerge CRDT document sync
//! between simulated peer instances.

use crate::state::document::*;
use dioxus::prelude::*;

/// Main Documents view component
#[component]
pub fn DocumentsView(
    state: Signal<DocumentState>,
    on_load_scenario: EventHandler<String>,
    on_run: EventHandler<()>,
    on_pause: EventHandler<()>,
    on_reset: EventHandler<()>,
    on_step: EventHandler<()>,
) -> Element {
    rsx! {
        div { class: "documents-view",
            // Scenario controls
            DocumentControls {
                state: state,
                on_load_scenario: on_load_scenario,
                on_run: on_run,
                on_pause: on_pause,
                on_reset: on_reset,
                on_step: on_step,
            }

            // Peer document cards
            PeerDocumentsGrid { state: state }

            // Bottom panels
            div { class: "document-panels",
                ConvergencePanel { state: state }
                DocumentEventTimeline { state: state }
            }
        }
    }
}

/// Grid of peer document cards
#[component]
pub fn PeerDocumentsGrid(state: Signal<DocumentState>) -> Element {
    let peer_names = state.read().peer_names();
    let is_empty = peer_names.is_empty();

    rsx! {
        div { class: "peer-documents-grid",
            for name in peer_names {
                if let Some(peer_state) = state.read().peers.get(&name) {
                    PeerDocumentCard { peer_state: peer_state.clone() }
                }
            }
            if is_empty {
                div { class: "no-peers-message",
                    "Load a scenario to see document states"
                }
            }
        }
    }
}

/// Card showing a single peer's document state
#[component]
pub fn PeerDocumentCard(peer_state: PeerDocumentState) -> Element {
    let head_preview = peer_state
        .heads
        .first()
        .map(|h| {
            let len = h.len().min(8);
            format!("{}...", &h[..len])
        })
        .unwrap_or_else(|| "none".to_string());

    rsx! {
        div { class: "peer-document-card",
            div { class: "peer-header",
                h4 { "{peer_state.peer_name}" }
                span { class: "head-preview", "Head: {head_preview}" }
            }
            div { class: "notes-list",
                if peer_state.notes.is_empty() {
                    div { class: "no-notes", "No notes" }
                } else {
                    for note in peer_state.notes.iter() {
                        div { class: "note-item",
                            div { class: "note-title", "{note.title}" }
                            if !note.content_preview.is_empty() {
                                div { class: "note-preview", "{note.content_preview}" }
                            }
                            div { class: "note-author", "by {note.author}" }
                        }
                    }
                }
            }
            div { class: "note-count", "{peer_state.note_count} notes" }
        }
    }
}

/// Convergence status panel
#[component]
pub fn ConvergencePanel(state: Signal<DocumentState>) -> Element {
    let is_converged = state.read().is_converged;
    let peers = state.read().peers.clone();
    let has_peers = !peers.is_empty();

    rsx! {
        div { class: "convergence-panel",
            h3 { class: "panel-title", "Sync Convergence" }
            if has_peers {
                div { class: if is_converged { "status converged" } else { "status divergent" },
                    if is_converged { "All peers converged" } else { "Peers divergent" }
                }
                div { class: "heads-list",
                    for (name, peer_state) in peers.iter() {
                        div { class: "head-entry",
                            span { class: "peer-name", "{name}: " }
                            span { class: "head-hash",
                                {peer_state.heads.first().map(|h| {
                                    let len = h.len().min(8);
                                    h[..len].to_string()
                                }).unwrap_or_else(|| "none".to_string())}
                            }
                        }
                    }
                }
            } else {
                div { class: "no-peers-message", "No peers loaded" }
            }
        }
    }
}

/// Event timeline for document operations
#[component]
pub fn DocumentEventTimeline(state: Signal<DocumentState>) -> Element {
    let events = state.read().events.clone();

    rsx! {
        div { class: "document-event-timeline",
            h3 { class: "panel-title", "Event Log" }
            div { class: "timeline-scroll",
                if events.is_empty() {
                    div { class: "no-events", "No events yet" }
                } else {
                    for event in events.iter().rev().take(50) {
                        DocumentEventItem { event: event.clone() }
                    }
                }
            }
        }
    }
}

/// Single event in timeline
#[component]
pub fn DocumentEventItem(event: DocumentEvent) -> Element {
    let (icon, class, text) = match &event {
        DocumentEvent::NoteCreated { peer, title, .. } => {
            ("+", "created", format!("{} created \"{}\"", peer, title))
        }
        DocumentEvent::NoteUpdated { peer, note_id, .. } => {
            let id_preview = if note_id.len() > 12 {
                &note_id[..12]
            } else {
                note_id
            };
            ("~", "updated", format!("{} updated {}", peer, id_preview))
        }
        DocumentEvent::NoteDeleted { peer, note_id, .. } => {
            let id_preview = if note_id.len() > 12 {
                &note_id[..12]
            } else {
                note_id
            };
            ("-", "deleted", format!("{} deleted {}", peer, id_preview))
        }
        DocumentEvent::SyncGenerated {
            from,
            to,
            size_bytes,
            ..
        } => (
            ">",
            "sync",
            format!("{} -> {} sync ({}B)", from, to, size_bytes),
        ),
        DocumentEvent::SyncApplied {
            peer,
            changes_applied,
            ..
        } => (
            "<",
            "sync",
            format!(
                "{} applied: {}",
                peer,
                if *changes_applied { "changes" } else { "no-op" }
            ),
        ),
        DocumentEvent::Converged { peers, .. } => {
            ("*", "converged", format!("Converged: {}", peers.join(", ")))
        }
        DocumentEvent::PhaseChanged { phase, .. } => ("#", "phase", phase.to_string()),
    };

    rsx! {
        div { class: "timeline-event {class}",
            span { class: "event-icon", "{icon}" }
            span { class: "event-text", "{text}" }
        }
    }
}

/// Scenario controls
#[component]
pub fn DocumentControls(
    state: Signal<DocumentState>,
    on_load_scenario: EventHandler<String>,
    on_run: EventHandler<()>,
    on_pause: EventHandler<()>,
    on_reset: EventHandler<()>,
    on_step: EventHandler<()>,
) -> Element {
    let running = state.read().running;
    let has_scenario = state.read().scenario_name.is_some();
    let current_step = state.read().current_step;
    let total_steps = state.read().total_steps;

    rsx! {
        div { class: "document-controls",
            div { class: "scenario-buttons",
                span { class: "control-label", "Load: " }
                button {
                    class: "control-btn secondary",
                    onclick: move |_| on_load_scenario.call("full_sync".to_string()),
                    "Full Sync Test"
                }
                button {
                    class: "control-btn secondary",
                    onclick: move |_| on_load_scenario.call("concurrent".to_string()),
                    "Concurrent Edits"
                }
                button {
                    class: "control-btn secondary",
                    onclick: move |_| on_load_scenario.call("offline".to_string()),
                    "Offline Sync"
                }
            }
            div { class: "playback-controls",
                button {
                    class: "control-btn",
                    disabled: !has_scenario || running,
                    onclick: move |_| on_step.call(()),
                    "Step"
                }
                button {
                    class: "control-btn",
                    disabled: !has_scenario,
                    onclick: move |_| {
                        if running {
                            on_pause.call(());
                        } else {
                            on_run.call(());
                        }
                    },
                    if running { "Pause" } else { "Run" }
                }
                button {
                    class: "control-btn secondary",
                    disabled: !has_scenario,
                    onclick: move |_| on_reset.call(()),
                    "Reset"
                }
                if has_scenario {
                    span { class: "step-counter", "Step {current_step} / {total_steps}" }
                }
            }
        }
    }
}
