//! Create/edit intention form with audience picker and token tag picker.

use dioxus::prelude::*;
use indras_network::member::MemberId;
use indras_sync_engine::{IntentionId, IntentionKind};

use crate::bridge::GiftCycleBridge;
use crate::data::TokenCardData;

/// Create new intention form.
#[component]
pub fn IntentionForm(
    bridge: GiftCycleBridge,
    available_tokens: Vec<TokenCardData>,
    connected_peers: Vec<MemberId>,
    on_created: EventHandler<IntentionId>,
    on_cancel: EventHandler<()>,
) -> Element {
    let mut title = use_signal(String::new);
    let mut description = use_signal(String::new);
    let mut selected_peers = use_signal(Vec::<MemberId>::new);
    let mut submitting = use_signal(|| false);
    let mut error_msg = use_signal(|| None::<String>);
    let mut selected_kind = use_signal(|| IntentionKind::Intention);

    let has_audience = !selected_peers.read().is_empty();

    rsx! {
        div { class: "intention-form",
            div { class: "form-header",
                h2 { class: "form-title", "New Intention" }
                button {
                    class: "gc-btn gc-btn-outline",
                    onclick: move |_| on_cancel.call(()),
                    "Cancel"
                }
            }

            div { class: "gc-field",
                label { class: "gc-label", "Title" }
                input {
                    class: "gc-input",
                    r#type: "text",
                    placeholder: "What do you need or offer?",
                    value: "{title}",
                    oninput: move |e| title.set(e.value()),
                }
            }

            div { class: "gc-field",
                label { class: "gc-label", "Kind" }
                div { class: "kind-picker",
                    for (kind, label) in [(IntentionKind::Need, "Need"), (IntentionKind::Offering, "Offering"), (IntentionKind::Intention, "Intention")] {
                        {
                            let k = kind.clone();
                            let is_active = selected_kind() == kind;
                            let class = if is_active { "kind-chip active" } else { "kind-chip" };
                            rsx! {
                                button {
                                    class: "{class}",
                                    r#type: "button",
                                    onclick: move |_| selected_kind.set(k.clone()),
                                    "{label}"
                                }
                            }
                        }
                    }
                }
            }

            div { class: "gc-field",
                label { class: "gc-label", "Description" }
                textarea {
                    class: "gc-textarea",
                    placeholder: "Describe your intention in detail...",
                    value: "{description}",
                    oninput: move |e| description.set(e.value()),
                }
            }

            // Audience picker
            if !connected_peers.is_empty() {
                div { class: "gc-field",
                    label { class: "gc-label", "Share with peers (optional)" }
                    div { class: "peer-picker",
                        for peer in &connected_peers {
                            {
                                let peer_id = *peer;
                                let peer_hex: String = peer_id.iter().take(4).map(|b| format!("{b:02x}")).collect();
                                let letter = peer_hex.chars().next().unwrap_or('?').to_string();
                                let is_selected = selected_peers.read().contains(&peer_id);
                                let class = if is_selected { "peer-chip selected" } else { "peer-chip" };
                                rsx! {
                                    div {
                                        class: "{class}",
                                        onclick: move |_| {
                                            let mut peers = selected_peers.write();
                                            if let Some(pos) = peers.iter().position(|p| *p == peer_id) {
                                                peers.remove(pos);
                                            } else {
                                                peers.push(peer_id);
                                            }
                                        },
                                        div { class: "peer-dot peer-dot-sage", "{letter}" }
                                        span { "{peer_hex}" }
                                    }
                                }
                            }
                        }
                    }
                    if has_audience {
                        div { class: "gc-hint", "Will be shared to {selected_peers.read().len()} peer DM realm(s)" }
                    } else {
                        div { class: "gc-hint", "Home realm only (visible to you)" }
                    }
                }
            }

            if let Some(err) = error_msg() {
                div { class: "form-error", "{err}" }
            }

            div { class: "form-actions",
                button {
                    class: "gc-btn gc-btn-primary",
                    disabled: title.read().trim().is_empty() || submitting(),
                    onclick: move |_| {
                        let b = bridge.clone();
                        let t = title.read().clone();
                        let d = description.read().clone();
                        let audience = selected_peers.read().clone();
                        let kind = selected_kind();
                        submitting.set(true);
                        error_msg.set(None);
                        spawn(async move {
                            let result = if audience.is_empty() {
                                b.create_intention(&t, &d, kind).await
                            } else {
                                b.create_dm_intention(&t, &d, kind, audience).await
                            };
                            match result {
                                Ok(id) => on_created.call(id),
                                Err(e) => {
                                    error_msg.set(Some(format!("{e}")));
                                    submitting.set(false);
                                }
                            }
                        });
                    },
                    if submitting() { "Posting..." } else { "\u{1f4a1} Post Intention" }
                }
            }
        }
    }
}
