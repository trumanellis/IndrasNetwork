//! Profile overlay — view and edit the local user's identity.

use std::sync::Arc;

use dioxus::prelude::*;
use indras_network::IndrasNetwork;

use crate::profile_bridge::{self, FieldVisibility, ALL_FIELDS};
use crate::state::{AppState, AppStep, default_data_dir, PEER_COLORS};

/// Truncate a hex string to `head…tail` form for display.
fn truncate_hex(s: &str) -> String {
    if s.len() <= 20 {
        s.to_string()
    } else {
        format!("{}…{}", &s[..10], &s[s.len() - 6..])
    }
}

/// Pick a stable color class for the avatar based on member id bytes.
fn avatar_color(member_id: &[u8; 32]) -> &'static str {
    let idx = (member_id[0] as usize) % PEER_COLORS.len();
    PEER_COLORS[idx]
}

/// Overlay modal for viewing and editing the local user's profile.
#[component]
pub fn ProfileOverlay(
    mut state: Signal<AppState>,
    network: Signal<Option<Arc<IndrasNetwork>>>,
) -> Element {
    if !state.read().show_profile {
        return rsx! {};
    }

    let display_name = state.read().display_name.clone();
    let mut draft = use_signal(|| display_name.clone());
    let mut copied = use_signal(|| false);
    let mut confirming_reset = use_signal(|| false);

    let net_ref = network.read().clone();
    let member_id = net_ref.as_ref().map(|n| n.id()).unwrap_or([0u8; 32]);
    let member_hex: String = member_id.iter().map(|b| format!("{:02x}", b)).collect();
    let member_display = truncate_hex(&member_hex);
    let avatar_class = avatar_color(&member_id);
    let avatar_letter = draft
        .read()
        .chars()
        .next()
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_else(|| "?".to_string());

    let device_count = state.read().device_count;
    let has_story = !state.read().pass_story_slots.is_empty();

    // Bio + per-field visibility come from the home-realm CRDT documents.
    // Loaded once per modal mount (component unmounts when show_profile=false).
    let mut bio_draft = use_signal(String::new);
    let mut visibilities: Signal<Vec<(&'static str, FieldVisibility)>> =
        use_signal(|| ALL_FIELDS.iter().map(|f| (*f, FieldVisibility::Private)).collect());

    let mut loaded = use_signal(|| false);
    use_effect(move || {
        if *loaded.read() {
            return;
        }
        let Some(net) = network.read().clone() else {
            return;
        };
        loaded.set(true);
        spawn(async move {
            if let Some(p) = profile_bridge::load_profile_identity(&net).await {
                bio_draft.set(p.bio.unwrap_or_default());
            }
            let v = profile_bridge::list_field_visibilities(&net).await;
            visibilities.set(v);
        });
    });

    let refresh_visibilities = move || {
        if let Some(net) = network.read().clone() {
            spawn(async move {
                let v = profile_bridge::list_field_visibilities(&net).await;
                visibilities.set(v);
            });
        }
    };

    let close = move |_| {
        state.write().show_profile = false;
    };

    rsx! {
        div {
            class: "file-modal-overlay",
            onclick: close,
            onkeydown: move |e: KeyboardEvent| {
                if e.key() == Key::Escape {
                    state.write().show_profile = false;
                }
            },

            div {
                class: "file-modal profile-modal",
                onclick: move |e| e.stop_propagation(),

                // Header
                div { class: "file-modal-header",
                    div { class: "relay-header-titles",
                        div { class: "relay-eyebrow", "YOU" }
                        div { class: "relay-title", "Profile" }
                    }
                    button {
                        class: "file-modal-close",
                        onclick: close,
                        "\u{00d7}"
                    }
                }

                // Body
                div { class: "file-modal-content relay-body",

                    // Identity: avatar + editable name
                    div { class: "profile-identity",
                        div { class: "profile-avatar {avatar_class}", "{avatar_letter}" }
                        input {
                            class: "profile-name-input",
                            r#type: "text",
                            value: "{draft}",
                            placeholder: "Your name",
                            autofocus: true,
                            oninput: move |e| draft.set(e.value()),
                            onblur: move |_| {
                                let trimmed = draft.read().trim().to_string();
                                if !trimmed.is_empty() {
                                    state.write().display_name = trimmed.clone();
                                    if let Some(net) = network.read().clone() {
                                        spawn(async move {
                                            crate::profile_bridge::save_display_name(&net, trimmed).await;
                                        });
                                    }
                                }
                            },
                            onkeydown: move |e: KeyboardEvent| {
                                if e.key() == Key::Enter {
                                    let trimmed = draft.read().trim().to_string();
                                    if !trimmed.is_empty() {
                                        state.write().display_name = trimmed.clone();
                                        if let Some(net) = network.read().clone() {
                                            spawn(async move {
                                                crate::profile_bridge::save_display_name(&net, trimmed).await;
                                            });
                                        }
                                    }
                                }
                            },
                        }
                    }

                    // Bio — inline-edit textarea (no panel wrapper so the edit surface is always obvious).
                    textarea {
                        class: "profile-bio-input",
                        placeholder: "A few words about you\u{2026}",
                        value: "{bio_draft}",
                        rows: "3",
                        oninput: move |e| bio_draft.set(e.value()),
                        onblur: move |_| {
                            let text = bio_draft.read().clone();
                            if let Some(net) = network.read().clone() {
                                spawn(async move {
                                    profile_bridge::save_bio(&net, text).await;
                                });
                            }
                        },
                    }

                    // Member ID
                    div { class: "relay-panel",
                        div { class: "relay-panel-header", "MEMBER ID" }
                        div { class: "relay-panel-body",
                            div { class: "relay-row",
                                span {
                                    class: "relay-id-value",
                                    title: "Click to copy",
                                    onclick: move |_| {
                                        let hex = member_hex.clone();
                                        #[cfg(target_os = "macos")]
                                        {
                                            let _ = std::process::Command::new("pbcopy")
                                                .arg(&hex)
                                                .stdin(std::process::Stdio::piped())
                                                .spawn()
                                                .and_then(|mut c| {
                                                    use std::io::Write;
                                                    if let Some(mut stdin) = c.stdin.take() {
                                                        let _ = stdin.write_all(hex.as_bytes());
                                                    }
                                                    c.wait()
                                                });
                                        }
                                        copied.set(true);
                                        spawn(async move {
                                            tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
                                            copied.set(false);
                                        });
                                    },
                                    "{member_display}"
                                }
                                if *copied.read() {
                                    span { class: "relay-copied-flash", "copied" }
                                }
                            }
                        }
                    }

                    // Devices
                    div { class: "relay-panel",
                        div { class: "relay-panel-header", "DEVICES" }
                        div { class: "relay-panel-body",
                            div { class: "relay-row",
                                span { class: "relay-row-label", "CONNECTED" }
                                span { class: "relay-row-value", "{device_count}" }
                            }
                        }
                    }

                    // Visibility — per-field grant controls
                    div { class: "relay-panel",
                        div { class: "relay-panel-header", "VISIBILITY" }
                        div { class: "relay-panel-body",
                            for (field_name, vis) in visibilities.read().iter().copied().collect::<Vec<_>>() {
                                div { class: "profile-vis-row",
                                    span { class: "profile-vis-label", "{profile_bridge::field_label(field_name)}" }
                                    div { class: "profile-vis-toggle",
                                        button {
                                            class: if vis == FieldVisibility::Public { "profile-vis-btn active" } else { "profile-vis-btn" },
                                            onclick: move |_| {
                                                if let Some(net) = network.read().clone() {
                                                    let f = field_name;
                                                    let mut refresh = refresh_visibilities;
                                                    spawn(async move {
                                                        profile_bridge::set_field_public(&net, f).await;
                                                        refresh();
                                                    });
                                                }
                                            },
                                            "Public"
                                        }
                                        button {
                                            class: if vis == FieldVisibility::ConnectionsOnly { "profile-vis-btn active" } else { "profile-vis-btn" },
                                            onclick: move |_| {
                                                if let Some(net) = network.read().clone() {
                                                    let f = field_name;
                                                    let mut refresh = refresh_visibilities;
                                                    spawn(async move {
                                                        profile_bridge::set_field_connections_only(&net, f).await;
                                                        refresh();
                                                    });
                                                }
                                            },
                                            "Contacts"
                                        }
                                        button {
                                            class: if vis == FieldVisibility::Private { "profile-vis-btn active" } else { "profile-vis-btn" },
                                            onclick: move |_| {
                                                if let Some(net) = network.read().clone() {
                                                    let f = field_name;
                                                    let mut refresh = refresh_visibilities;
                                                    spawn(async move {
                                                        profile_bridge::set_field_private(&net, f).await;
                                                        refresh();
                                                    });
                                                }
                                            },
                                            "Private"
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Recovery story
                    div { class: "relay-panel",
                        div { class: "relay-panel-header", "RECOVERY STORY" }
                        div { class: "relay-panel-body",
                            if has_story {
                                div { class: "profile-hint",
                                    "Your pass story is the only way to recover this identity on a new device. Keep it somewhere private."
                                }
                            } else {
                                div { class: "profile-hint warn",
                                    "No recovery story on file. Without one, this identity cannot be restored."
                                }
                            }
                        }
                    }

                    // Danger zone
                    div { class: "relay-panel profile-danger",
                        div { class: "relay-panel-header", "DANGER" }
                        div { class: "relay-panel-body",
                            if *confirming_reset.read() {
                                div { class: "profile-hint warn",
                                    "This will erase local identity and all vault data on this device."
                                }
                                div { class: "profile-danger-actions",
                                    button {
                                        class: "se-btn-outline",
                                        onclick: move |_| confirming_reset.set(false),
                                        "Cancel"
                                    }
                                    button {
                                        class: "se-btn-danger",
                                        onclick: move |_| {
                                            let data_dir = default_data_dir();
                                            let _ = std::fs::remove_dir_all(&data_dir);
                                            state.write().show_profile = false;
                                            state.write().display_name = String::new();
                                            state.write().pass_story_slots = Vec::new();
                                            state.write().private_files = Vec::new();
                                            state.write().realms = Vec::new();
                                            state.write().step = AppStep::Welcome;
                                        },
                                        "Erase & sign out"
                                    }
                                }
                            } else {
                                button {
                                    class: "se-btn-danger-outline",
                                    onclick: move |_| confirming_reset.set(true),
                                    "Sign out & reset identity"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
