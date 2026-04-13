//! Relay-node settings overlay — frictionless inline editing.
//!
//! Reuses `.file-modal-overlay` / `.file-modal` chrome from `file_modal`.
//! All edits autosave on blur; no save / apply / confirm buttons.

use dioxus::prelude::*;

use crate::config::PRESET_NAMES;
use crate::state::AppState;

/// Truncate a hex string to `head…tail` form for display.
fn truncate_hex(s: &str) -> String {
    if s.len() <= 20 {
        s.to_string()
    } else {
        format!("{}…{}", &s[..10], &s[s.len() - 6..])
    }
}

/// Persist the cached `relay_config` to disk.
fn persist(state: &Signal<AppState>) {
    let cfg = state.read().relay_config.clone();
    let _ = cfg.save();
}

/// Overlay component for viewing and editing relay-node configuration.
#[component]
pub fn RelaySettingsOverlay(mut state: Signal<AppState>) -> Element {
    if !state.read().show_relay_settings {
        return rsx! {};
    }

    let mut copied = use_signal(|| false);
    let mut ghost_draft = use_signal(String::new);

    // Placeholder local peer ID. Real ID will come from the network handle once
    // wired; for now, derive a stable label from the data dir path so the
    // surface is visible and click-to-copy works end-to-end.
    let peer_id = "local-peer-id-pending".to_string();
    let peer_id_display = truncate_hex(&peer_id);

    let close = move |_| {
        state.write().show_relay_settings = false;
    };

    let cfg = state.read().relay_config.clone();
    let local_only = cfg.local_only;
    let preset = cfg.preset.clone();
    let servers = cfg.servers.clone();

    rsx! {
        div {
            class: "file-modal-overlay",
            onclick: close,
            onkeydown: move |e: KeyboardEvent| {
                if e.key() == Key::Escape {
                    state.write().show_relay_settings = false;
                }
            },

            div {
                class: "file-modal relay-settings",
                onclick: move |e| e.stop_propagation(),

                // Header
                div { class: "file-modal-header",
                    div { class: "relay-header-titles",
                        div { class: "relay-eyebrow", "NETWORK · SETTINGS" }
                        div { class: "relay-title", "Relay Node" }
                    }
                    button {
                        class: "file-modal-close",
                        onclick: close,
                        "\u{00d7}"
                    }
                }

                // Body
                div { class: "file-modal-content relay-body",

                    // Identity panel
                    div { class: "relay-panel",
                        div { class: "relay-panel-header", "IDENTITY" }
                        div { class: "relay-panel-body",
                            div { class: "relay-row",
                                span { class: "relay-row-label", "PEER" }
                                span {
                                    class: "relay-id-value",
                                    title: "Click to copy",
                                    onclick: move |_| {
                                        copied.set(true);
                                        spawn(async move {
                                            tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
                                            copied.set(false);
                                        });
                                    },
                                    "{peer_id_display}"
                                }
                                if *copied.read() {
                                    span { class: "relay-copied-flash", "copied" }
                                }
                            }

                            // Preset selector — segmented pills
                            div { class: "relay-row relay-preset-row",
                                span { class: "relay-row-label", "PRESET" }
                                div { class: "relay-preset-group",
                                    for name in PRESET_NAMES.iter() {
                                        {
                                            let is_active = preset == *name;
                                            let n = (*name).to_string();
                                            rsx! {
                                                button {
                                                    key: "{name}",
                                                    class: if is_active { "relay-preset-pill active" } else { "relay-preset-pill" },
                                                    onclick: move |_| {
                                                        state.write().relay_config.preset = n.clone();
                                                        persist(&state);
                                                    },
                                                    "{name}"
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Relay mode toggle
                    div { class: "relay-panel",
                        div { class: "relay-panel-body",
                            div { class: "relay-row relay-toggle-row",
                                div { class: "relay-toggle-text",
                                    div { class: "relay-toggle-label", "Use public relays" }
                                    div { class: "relay-toggle-hint",
                                        if local_only {
                                            "LAN-only: peers must be on the same network"
                                        } else {
                                            "Public relays enable peers to find each other anywhere"
                                        }
                                    }
                                }
                                button {
                                    class: if !local_only { "relay-toggle on" } else { "relay-toggle" },
                                    onclick: move |_| {
                                        let new_val = !state.read().relay_config.local_only;
                                        state.write().relay_config.local_only = new_val;
                                        persist(&state);
                                    },
                                    span { class: "relay-toggle-knob" }
                                }
                            }
                            div { class: "relay-restart-note",
                                span { class: "relay-restart-dot" }
                                "active on restart"
                            }
                        }
                    }

                    // Relay servers list
                    div { class: "relay-panel",
                        div { class: "relay-panel-header", "RELAY SERVERS" }
                        div { class: "relay-panel-body relay-server-list",
                            for (idx, url) in servers.iter().enumerate() {
                                {
                                    let url_owned = url.clone();
                                    rsx! {
                                        div {
                                            key: "{idx}",
                                            class: "relay-server-row",
                                            input {
                                                class: "relay-server-input",
                                                r#type: "text",
                                                value: "{url_owned}",
                                                onchange: move |e| {
                                                    let v = e.value().trim().to_string();
                                                    if v.is_empty() {
                                                        state.write().relay_config.servers.remove(idx);
                                                    } else {
                                                        state.write().relay_config.servers[idx] = v;
                                                    }
                                                    persist(&state);
                                                },
                                            }
                                            button {
                                                class: "relay-server-remove",
                                                title: "Remove relay",
                                                onclick: move |_| {
                                                    state.write().relay_config.servers.remove(idx);
                                                    persist(&state);
                                                },
                                                "\u{00d7}"
                                            }
                                        }
                                    }
                                }
                            }

                            // Ghost-add row
                            div { class: "relay-server-row relay-server-ghost-row",
                                input {
                                    class: "relay-server-input relay-server-ghost",
                                    r#type: "text",
                                    placeholder: "+ add relay server",
                                    value: "{ghost_draft}",
                                    oninput: move |e| ghost_draft.set(e.value()),
                                    onchange: move |e| {
                                        let v = e.value().trim().to_string();
                                        if !v.is_empty() {
                                            state.write().relay_config.servers.push(v);
                                            persist(&state);
                                            ghost_draft.set(String::new());
                                        }
                                    },
                                }
                            }
                        }
                    }

                    // Status footer
                    div { class: "relay-status-footer",
                        if servers.is_empty() {
                            span { class: "relay-status-empty", "No relays configured" }
                        } else {
                            for (idx, url) in servers.iter().enumerate() {
                                div {
                                    key: "{idx}",
                                    class: "relay-status-line",
                                    span { class: "relay-status-dot" }
                                    span { class: "relay-status-url", "{url}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

