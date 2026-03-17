//! Profile visibility grant management panel.

use dioxus::prelude::*;

use crate::bridge::GiftCycleBridge;
use crate::data::{FieldVisibility, ProfileFieldVisibility};

/// Profile grants management panel.
#[component]
pub fn ProfileGrantsPanel(
    bridge: GiftCycleBridge,
    profile_fields: Vec<ProfileFieldVisibility>,
    on_back: EventHandler<()>,
) -> Element {
    rsx! {
        div { class: "profile-grants",
            div { class: "feed-header",
                h2 { class: "feed-title", "Profile Visibility" }
                button {
                    class: "gc-btn gc-btn-outline",
                    onclick: move |_| on_back.call(()),
                    "\u{2190} Back"
                }
            }

            if profile_fields.is_empty() {
                div { class: "feed-empty",
                    "No profile fields configured."
                }
            }

            for field in &profile_fields {
                FieldVisibilityRow {
                    field: field.clone(),
                    bridge: bridge.clone(),
                }
            }
        }
    }
}

/// A single field row with visibility toggle.
#[component]
fn FieldVisibilityRow(field: ProfileFieldVisibility, bridge: GiftCycleBridge) -> Element {
    let mut expanded = use_signal(|| false);
    let field_name = field.field_name.clone();
    let has_grants = !field.specific_grants.is_empty();

    // Truncate display value for long fields (like JSON arrays)
    let short_value = if field.display_value.len() > 40 {
        format!("{}\u{2026}", &field.display_value[..37])
    } else {
        field.display_value.clone()
    };

    rsx! {
        div { class: "field-row",
            div { class: "field-info",
                span { class: "field-label", "{field.display_label}" }
                span { class: "field-value", "{short_value}" }
            }
            div { class: "field-controls",
                // Three-way toggle
                {
                    let b1 = bridge.clone();
                    let fn1 = field_name.clone();
                    let b2 = bridge.clone();
                    let fn2 = field_name.clone();
                    let b3 = bridge.clone();
                    let fn3 = field_name.clone();
                    rsx! {
                        div { class: "visibility-toggle",
                            button {
                                class: if matches!(field.visibility, FieldVisibility::Public) { "toggle-btn active" } else { "toggle-btn" },
                                onclick: move |_| {
                                    let b = b1.clone();
                                    let f = fn1.clone();
                                    spawn(async move {
                                        let _ = b.set_field_public(&f).await;
                                    });
                                },
                                "Public"
                            }
                            button {
                                class: if matches!(field.visibility, FieldVisibility::ConnectionsOnly) { "toggle-btn active" } else { "toggle-btn" },
                                onclick: move |_| {
                                    let b = b2.clone();
                                    let f = fn2.clone();
                                    spawn(async move {
                                        let _ = b.set_field_connections_only(&f).await;
                                    });
                                },
                                "Contacts"
                            }
                            button {
                                class: if matches!(field.visibility, FieldVisibility::Private) { "toggle-btn active" } else { "toggle-btn" },
                                onclick: move |_| {
                                    let b = b3.clone();
                                    let f = fn3.clone();
                                    spawn(async move {
                                        let _ = b.set_field_private(&f).await;
                                    });
                                },
                                "Private"
                            }
                        }
                    }
                }
                // Expand button for specific grants
                if has_grants {
                    button {
                        class: "gc-btn gc-btn-outline expand-btn",
                        onclick: move |_| {
                            let current = *expanded.read();
                            expanded.set(!current);
                        },
                        if *expanded.read() { "\u{25b2}" } else { "\u{25bc}" }
                    }
                }
            }
        }
        // Expanded grants list
        if *expanded.read() && has_grants {
            div { class: "grants-list",
                for grant in &field.specific_grants {
                    {
                        let grantee = grant.grantee;
                        let b = bridge.clone();
                        let fn_name = field_name.clone();
                        rsx! {
                            div { class: "grant-item",
                                span { class: "grant-name", "{grant.grantee_name}" }
                                span { class: "grant-mode", "{grant.mode_label}" }
                                button {
                                    class: "gc-btn gc-btn-danger revoke-btn",
                                    onclick: move |_| {
                                        let b = b.clone();
                                        let f = fn_name.clone();
                                        spawn(async move {
                                            let _ = b.revoke_field_access(&f, grantee).await;
                                        });
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
