//! Profile overlay — view and edit the local user's identity.

use std::collections::HashSet;
use std::sync::Arc;

use dioxus::prelude::*;
use indras_network::IndrasNetwork;

use super::markdown_editor::InlineMarkdownEditor;
use crate::profile_bridge::{self, FieldVisibility, ProfileFieldVisibility, ALL_FIELDS};
use crate::state::{AppState, AppStep, default_data_dir, MEMBER_IDENTITY_CLASSES};

/// Well-known bio file name inside the home vault.
const BIO_FILENAME: &str = "BIO.md";

/// UI feedback state for inline field saves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SaveFeedback {
    /// No save in flight or recently finished.
    Idle,
    /// Save dispatched, awaiting confirmation.
    Saving,
    /// Recently saved; indicator flashes briefly.
    Saved,
}

fn save_indicator_text(state: SaveFeedback) -> &'static str {
    match state {
        SaveFeedback::Idle => "",
        SaveFeedback::Saving => "saving…",
        SaveFeedback::Saved => "saved",
    }
}

/// Spawn a save future with feedback wiring: flips the signal to `Saving`
/// immediately, `Saved` when the future resolves, then back to `Idle` after
/// a short delay so the indicator flashes briefly.
fn dispatch_save<F, Fut>(mut state: Signal<SaveFeedback>, f: F)
where
    F: FnOnce() -> Fut + 'static,
    Fut: std::future::Future<Output = ()> + 'static,
{
    state.set(SaveFeedback::Saving);
    spawn(async move {
        f().await;
        state.set(SaveFeedback::Saved);
        tokio::time::sleep(std::time::Duration::from_millis(1400)).await;
        if *state.read() == SaveFeedback::Saved {
            state.set(SaveFeedback::Idle);
        }
    });
}

/// Tiny flash indicator showing current save state for an inline-editable field.
#[component]
fn SaveIndicator(state: Signal<SaveFeedback>) -> Element {
    let text = save_indicator_text(*state.read());
    if text.is_empty() {
        return rsx! {};
    }
    rsx! { span { class: "profile-save-indicator", "{text}" } }
}

/// Truncate a hex string to `head…tail` form for display.
fn truncate_hex(s: &str) -> String {
    if s.len() <= 20 {
        s.to_string()
    } else {
        format!("{}…{}", &s[..10], &s[s.len() - 6..])
    }
}

/// Pick a stable Member Identity Color class for the avatar based on member id bytes.
fn avatar_color(member_id: &[u8; 32]) -> &'static str {
    let idx = (member_id[0] as usize) % MEMBER_IDENTITY_CLASSES.len();
    MEMBER_IDENTITY_CLASSES[idx]
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

    // Resolve the user's bio file path and make sure it exists on disk so the
    // Milkdown editor has something to load. Creating an empty file synchronously
    // avoids a race where the editor mounts before an async seed task runs.
    let vault_path = state.read().vault_path.clone();
    let bio_path = vault_path.join(BIO_FILENAME);
    if !bio_path.exists() {
        if let Some(parent) = bio_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&bio_path, "");
    }

    // Callback fires on every Milkdown save: mirrors BIO.md contents into the
    // CRDT `bio` field so the visibility panel and peer views stay fresh.
    let bio_callback: Option<Callback<String>> = net_ref.as_ref().map(|net| {
        let net = net.clone();
        Callback::new(move |content: String| {
            let net = net.clone();
            spawn(async move {
                profile_bridge::save_bio(&net, content).await;
            });
        })
    });

    // Username draft starts as `None` until the CRDT load resolves — this
    // prevents the async load from clobbering keystrokes typed by an early user.
    let mut username_draft: Signal<Option<String>> = use_signal(|| None);
    // Seed with a Private placeholder for every known field so the user sees
    // all 12 rows immediately while the async load resolves real state.
    let mut visibilities: Signal<Vec<ProfileFieldVisibility>> = use_signal(|| {
        ALL_FIELDS
            .iter()
            .map(|f| ProfileFieldVisibility {
                field_name: *f,
                display_label: profile_bridge::field_label(f),
                display_value: String::new(),
                visibility: FieldVisibility::Private,
                specific_grants: Vec::new(),
            })
            .collect()
    });

    // Save-state indicators per editable field.
    let name_save = use_signal(|| SaveFeedback::Idle);
    let username_save = use_signal(|| SaveFeedback::Idle);

    // Set of field names whose per-grantee list is currently expanded.
    let mut expanded: Signal<HashSet<&'static str>> = use_signal(HashSet::new);

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
                username_draft.set(Some(p.username));
            } else {
                username_draft.set(Some(String::new()));
            }
            let v = profile_bridge::list_field_visibilities(&net).await;
            tracing::info!(count = v.len(), "profile visibilities loaded");
            visibilities.set(v);
        });
    });

    let refresh_visibilities = move || {
        if let Some(net) = network.read().clone() {
            spawn(async move {
                let v = profile_bridge::list_field_visibilities(&net).await;
                tracing::info!(count = v.len(), "profile visibilities refreshed");
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
                    div { class: "profile-header-titles",
                        div { class: "profile-eyebrow", "YOU" }
                        div { class: "profile-title", "Profile" }
                    }
                    button {
                        class: "file-modal-close",
                        onclick: close,
                        "\u{00d7}"
                    }
                }

                // Body
                div { class: "file-modal-content profile-body",

                    // Identity: avatar + editable name + username
                    div { class: "profile-identity",
                        div { class: "profile-avatar {avatar_class}", "{avatar_letter}" }
                        div { class: "profile-identity-fields",
                            div { class: "profile-input-row",
                                input {
                                    class: "profile-name-input",
                                    r#type: "text",
                                    value: "{draft}",
                                    placeholder: "Your name",
                                    autofocus: true,
                                    oninput: move |e| draft.set(e.value()),
                                    onblur: move |_| {
                                        let trimmed = draft.read().trim().to_string();
                                        if trimmed.is_empty() { return; }
                                        state.write().display_name = trimmed.clone();
                                        let Some(net) = network.read().clone() else { return };
                                        dispatch_save(name_save, move || {
                                            let trimmed = trimmed.clone();
                                            async move {
                                                crate::profile_bridge::save_display_name(&net, trimmed).await;
                                            }
                                        });
                                    },
                                    onkeydown: move |e: KeyboardEvent| {
                                        if e.key() != Key::Enter { return; }
                                        let trimmed = draft.read().trim().to_string();
                                        if trimmed.is_empty() { return; }
                                        state.write().display_name = trimmed.clone();
                                        let Some(net) = network.read().clone() else { return };
                                        dispatch_save(name_save, move || {
                                            let trimmed = trimmed.clone();
                                            async move {
                                                crate::profile_bridge::save_display_name(&net, trimmed).await;
                                            }
                                        });
                                    },
                                }
                                SaveIndicator { state: name_save }
                            }
                            if let Some(username) = username_draft.read().clone() {
                                div { class: "profile-input-row",
                                    span { class: "profile-handle-prefix", "@" }
                                    input {
                                        class: "profile-handle-input",
                                        r#type: "text",
                                        value: "{username}",
                                        placeholder: "handle",
                                        oninput: move |e| username_draft.set(Some(e.value())),
                                        onblur: move |_| {
                                            let Some(value) = username_draft.read().clone() else { return };
                                            let Some(net) = network.read().clone() else { return };
                                            dispatch_save(username_save, move || {
                                                let value = value.clone();
                                                async move {
                                                    crate::profile_bridge::save_username(&net, value).await;
                                                }
                                            });
                                        },
                                    }
                                    SaveIndicator { state: username_save }
                                }
                            }
                        }
                    }

                    // Bio — WYSIWYG Milkdown editor backed by BIO.md in the home vault.
                    // Content auto-saves to disk; the callback mirrors each save into the
                    // CRDT `bio` field so the visibility panel stays in sync.
                    div { class: "profile-bio-wrap",
                        InlineMarkdownEditor {
                            full_path: bio_path.clone(),
                            on_content: bio_callback,
                        }
                    }

                    // Inline identity strip: member ID (click to copy) · device count
                    div { class: "profile-meta-strip",
                        span {
                            class: "profile-id-value",
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
                        span { class: "profile-meta-sep", "\u{00b7}" }
                        span { class: "profile-meta-devices",
                            "{device_count} "
                            if device_count == 1 { "device" } else { "devices" }
                        }
                        if *copied.read() {
                            span { class: "profile-copied-flash", "copied" }
                        }
                    }

                    // Visibility — per-field grant controls with expandable per-grantee lists.
                    // Row rendering is inlined (not extracted to a child component) because
                    // Dioxus 0.7's `#[component]` in a `for` loop has a memoization quirk
                    // that dropped all but the first few instances in this context.
                    div { class: "profile-panel",
                        div { class: "profile-panel-header", "VISIBILITY" }
                        div { class: "profile-panel-body",
                            for field in visibilities.read().clone() {
                                {
                                    let field_name: &'static str = field.field_name;
                                    let short_value = truncate_value(&field.display_value);
                                    let has_grants = !field.specific_grants.is_empty();
                                    let is_public = field.visibility == FieldVisibility::Public;
                                    let is_contacts = field.visibility == FieldVisibility::ConnectionsOnly;
                                    let is_private = field.visibility == FieldVisibility::Private;
                                    let is_expanded = expanded.read().contains(field_name);
                                    rsx! {
                                        div { class: "profile-field-row", key: "{field_name}",
                                            div { class: "profile-field-info",
                                                span { class: "profile-field-label", "{field.display_label}" }
                                                if !short_value.is_empty() {
                                                    span { class: "profile-field-value", "{short_value}" }
                                                }
                                            }
                                            div { class: "profile-field-controls",
                                                div { class: "profile-vis-toggle",
                                                    button {
                                                        class: if is_public { "profile-vis-btn active" } else { "profile-vis-btn" },
                                                        onclick: move |_| {
                                                            let Some(net) = network.read().clone() else { return };
                                                            spawn(async move {
                                                                profile_bridge::set_field_public(&net, field_name).await;
                                                                refresh_visibilities();
                                                            });
                                                        },
                                                        "Public"
                                                    }
                                                    button {
                                                        class: if is_contacts { "profile-vis-btn active" } else { "profile-vis-btn" },
                                                        onclick: move |_| {
                                                            let Some(net) = network.read().clone() else { return };
                                                            spawn(async move {
                                                                profile_bridge::set_field_connections_only(&net, field_name).await;
                                                                refresh_visibilities();
                                                            });
                                                        },
                                                        "Contacts"
                                                    }
                                                    button {
                                                        class: if is_private { "profile-vis-btn active" } else { "profile-vis-btn" },
                                                        onclick: move |_| {
                                                            let Some(net) = network.read().clone() else { return };
                                                            spawn(async move {
                                                                profile_bridge::set_field_private(&net, field_name).await;
                                                                refresh_visibilities();
                                                            });
                                                        },
                                                        "Private"
                                                    }
                                                }
                                                if has_grants {
                                                    button {
                                                        class: "profile-expand-btn",
                                                        onclick: move |_| {
                                                            let mut set = expanded.write();
                                                            if set.contains(field_name) {
                                                                set.remove(field_name);
                                                            } else {
                                                                set.insert(field_name);
                                                            }
                                                        },
                                                        if is_expanded { "\u{25b2}" } else { "\u{25bc}" }
                                                    }
                                                }
                                            }
                                        }
                                        if is_expanded && has_grants {
                                            div { class: "profile-grants-list",
                                                for grant in field.specific_grants.iter().cloned() {
                                                    div {
                                                        class: "profile-grant-item",
                                                        key: "{hex_grantee(&grant.grantee)}",
                                                        span { class: "profile-grant-name", "{grant.grantee_name}" }
                                                        span { class: "profile-grant-mode", "{grant.mode_label}" }
                                                        button {
                                                            class: "profile-revoke-btn",
                                                            onclick: {
                                                                let grantee = grant.grantee;
                                                                move |_| {
                                                                    let Some(net) = network.read().clone() else { return };
                                                                    spawn(async move {
                                                                        profile_bridge::revoke_field_access(&net, field_name, grantee).await;
                                                                        refresh_visibilities();
                                                                    });
                                                                }
                                                            },
                                                            "Revoke"
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Recovery story
                    div { class: "profile-panel",
                        div { class: "profile-panel-header", "RECOVERY STORY" }
                        div { class: "profile-panel-body",
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
                    div { class: "profile-panel profile-danger",
                        div { class: "profile-panel-header", "DANGER" }
                        div { class: "profile-panel-body",
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

/// Truncate a field's display value so long entries (JSON arrays, long bios)
/// don't blow out the row width.
fn truncate_value(s: &str) -> String {
    if s.chars().count() <= 40 {
        s.to_string()
    } else {
        let head: String = s.chars().take(37).collect();
        format!("{head}\u{2026}")
    }
}

fn hex_grantee(id: &[u8; 32]) -> String {
    id.iter().take(6).map(|b| format!("{b:02x}")).collect()
}
