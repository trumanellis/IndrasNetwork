//! Quest/Need/Offering/Intention view component.

use dioxus::prelude::*;
use indras_ui::{ArtifactDisplayInfo, ArtifactGallery};

/// The kind of quest-like artifact.
#[derive(Clone, Debug, PartialEq)]
pub enum QuestKind {
    Quest,
    Need,
    Offering,
    Intention,
}

impl QuestKind {
    pub fn css_class(&self) -> &str {
        match self {
            QuestKind::Quest => "type-quest",
            QuestKind::Need => "type-need",
            QuestKind::Offering => "type-offering",
            QuestKind::Intention => "type-intention",
        }
    }
    pub fn icon(&self) -> &str {
        match self {
            QuestKind::Quest => "\u{2694}",
            QuestKind::Need => "\u{1F331}",
            QuestKind::Offering => "\u{1F381}",
            QuestKind::Intention => "\u{2728}",
        }
    }
    pub fn label(&self) -> &str {
        match self {
            QuestKind::Quest => "Quest",
            QuestKind::Need => "Need",
            QuestKind::Offering => "Offering",
            QuestKind::Intention => "Intention",
        }
    }
}

/// An artifact attachment on a proof card.
#[derive(Clone, Debug, PartialEq)]
pub struct ProofArtifact {
    pub icon: String,
    pub name: String,
    pub artifact_type: String,
}

/// An assigned token on a proof card.
#[derive(Clone, Debug, PartialEq)]
pub struct AssignedToken {
    pub duration: String,
    pub source: String,
}

/// A proof of service entry.
#[derive(Clone, Debug, PartialEq)]
pub struct ProofEntry {
    pub author_name: String,
    pub author_letter: String,
    pub author_color_class: String,
    pub body: String,
    pub time_ago: String,
    pub artifact_attachments: Vec<ProofArtifact>,
    pub tokens: Vec<AssignedToken>,
    pub has_tokens: bool,
    pub total_token_count: usize,
    pub total_token_duration: String,
}

/// An attention item for the token picker.
#[derive(Clone, Debug, PartialEq)]
pub struct AttentionItem {
    pub target: String,
    pub when: String,
    pub duration: String,
}

/// Individual proof card component — avoids lifetime issues with EventHandler.
#[component]
fn ProofCard(
    proof: ProofEntry,
    on_assign: Option<EventHandler<()>>,
) -> Element {
    let card_class = if proof.has_tokens { "proof-card has-tokens" } else { "proof-card" };
    let assign_label = if proof.has_tokens { "+ Assign more" } else { "+ Assign" };

    rsx! {
        div {
            class: "{card_class}",
            div {
                class: "proof-card-header",
                div {
                    class: "proof-card-avatar {proof.author_color_class}",
                    "{proof.author_letter}"
                }
                div { class: "proof-card-from", "{proof.author_name}" }
                div { class: "proof-card-time", "{proof.time_ago}" }
            }
            div { class: "proof-card-body", "{proof.body}" }
            if proof.artifact_attachments.len() == 1 {
                {
                    let a = &proof.artifact_attachments[0];
                    let info = ArtifactDisplayInfo {
                        name: a.name.clone(),
                        mime_type: if a.artifact_type == "Image" { Some("image/png".into()) } else { None },
                        ..Default::default()
                    };
                    let icon = info.icon();
                    rsx! {
                        div {
                            class: "proof-artifact",
                            span { class: "proof-artifact-icon", "{icon}" }
                            span { class: "proof-artifact-name", "{a.name}" }
                            span { class: "proof-artifact-type", "{a.artifact_type}" }
                        }
                    }
                }
            } else if proof.artifact_attachments.len() > 1 {
                {
                    let gallery_items: Vec<ArtifactDisplayInfo> = proof.artifact_attachments.iter().enumerate().map(|(i, a)| {
                        ArtifactDisplayInfo {
                            id: format!("proof-{i}"),
                            name: a.name.clone(),
                            mime_type: if a.artifact_type == "Image" { Some("image/png".into()) } else { None },
                            ..Default::default()
                        }
                    }).collect();
                    rsx! {
                        ArtifactGallery { artifacts: gallery_items }
                    }
                }
            }
            div {
                class: "proof-tokens",
                div {
                    class: "proof-tokens-header",
                    div {
                        class: "proof-tokens-title",
                        span { class: "token-icon", "\u{2728}" }
                        " Tokens of Gratitude"
                    }
                    if let Some(handler) = on_assign {
                        button {
                            class: "assign-btn",
                            onclick: move |_| handler.call(()),
                            "{assign_label}"
                        }
                    }
                }
                if proof.tokens.is_empty() {
                    div {
                        class: "no-tokens-yet",
                        "No attention artifacts assigned yet"
                    }
                } else {
                    div {
                        class: "assigned-tokens",
                        for token in proof.tokens.iter() {
                            div {
                                class: "assigned-token",
                                span { class: "token-time-pill", "{token.duration}" }
                                span { class: "token-source", "\u{25B6} {token.source}" }
                                button {
                                    class: "token-remove",
                                    title: "Unassign",
                                    "\u{2715}"
                                }
                            }
                        }
                    }
                    div {
                        style: "margin-top:8px;font-size:11px;color:var(--text-muted);display:flex;justify-content:space-between",
                        span { "{proof.total_token_count} attention artifacts assigned" }
                        span {
                            style: "color:var(--accent-gold);font-weight:500;font-family:var(--font-mono)",
                            "{proof.total_token_duration} total"
                        }
                    }
                }
            }
        }
    }
}

/// Token picker modal component.
#[component]
fn TokenPickerModal(
    items: Vec<AttentionItem>,
    on_close: Option<EventHandler<()>>,
) -> Element {
    rsx! {
        div {
            class: "token-picker visible",
            div {
                class: "token-picker-panel",
                div {
                    class: "token-picker-header",
                    div { class: "token-picker-title", "Assign Attention" }
                    if let Some(handler) = on_close {
                        button {
                            class: "token-picker-close",
                            onclick: move |_| handler.call(()),
                            "\u{2715}"
                        }
                    }
                }
                div {
                    class: "token-picker-subtitle",
                    "Select attention artifacts from your log. Each represents time you spent navigating to an artifact \u{2014} that dwell time becomes the token\u{2019}s value."
                }
                div {
                    class: "token-picker-list",
                    for item in items.iter() {
                        div {
                            class: "attn-row",
                            div { class: "attn-check", "\u{2713}" }
                            div {
                                class: "attn-info",
                                div { class: "attn-target", "{item.target}" }
                                div { class: "attn-when", "{item.when}" }
                            }
                            div { class: "attn-duration", "{item.duration}" }
                        }
                    }
                }
                div {
                    class: "token-picker-footer",
                    div {
                        class: "picker-total",
                        strong { "0" }
                        " selected \u{00B7} "
                        strong { "0m 0s" }
                    }
                    button {
                        class: "picker-confirm",
                        disabled: true,
                        "Assign Tokens"
                    }
                }
            }
        }
    }
}

/// Quest view — hero card for Quest/Need/Offering/Intention.
#[component]
pub fn QuestView(
    kind: QuestKind,
    title: String,
    description: String,
    status: String,
    steward_name: String,
    audience_count: usize,
    proofs: Vec<ProofEntry>,
    #[props(default = String::new())]
    posted_ago: String,
    #[props(default = false)]
    token_picker_open: bool,
    on_open_token_picker: Option<EventHandler<()>>,
    on_close_token_picker: Option<EventHandler<()>>,
    #[props(default = Vec::new())]
    attention_items: Vec<AttentionItem>,
) -> Element {
    let card_class = format!("quest-card {}", kind.css_class());
    let status_class = format!("quest-status {}", status.to_lowercase());
    let audience_text = format!("{} audience", audience_count);
    let proofs_count = format!("{}", proofs.len());
    let total_tokens: usize = proofs.iter().map(|p| p.total_token_count).sum();
    let token_meta = if total_tokens > 0 {
        format!("\u{2728} {} tokens assigned", total_tokens)
    } else {
        String::new()
    };

    rsx! {
        div {
            class: "view active",
            div {
                class: "request-scroll",
                div {
                    class: "request-layout",
                    // Quest card
                    div {
                        class: "{card_class}",
                        div {
                            class: "quest-header",
                            div { class: "quest-icon", "{kind.icon()}" }
                            div { class: "quest-title", "{title}" }
                            div { class: "{status_class}", "{status}" }
                        }
                        div { class: "quest-description", "{description}" }
                        div {
                            class: "quest-meta",
                            div {
                                class: "quest-meta-item",
                                span { style: "color:var(--accent-teal)", "\u{25CF}" }
                                " Steward: {steward_name}"
                            }
                            div {
                                class: "quest-meta-item",
                                "\u{1F465} {audience_text}"
                            }
                            div {
                                class: "quest-meta-item",
                                "\u{1F4DC} {proofs_count} proofs"
                            }
                            if !token_meta.is_empty() {
                                div {
                                    class: "quest-meta-item",
                                    "{token_meta}"
                                }
                            }
                            if !posted_ago.is_empty() {
                                div {
                                    class: "quest-meta-item",
                                    "\u{1F551} {posted_ago}"
                                }
                            }
                        }
                    }
                    // Proofs section
                    if !proofs.is_empty() {
                        div {
                            class: "proofs-header",
                            div {
                                class: "proofs-title",
                                "Proofs of Service "
                                span { class: "proofs-count", "{proofs_count}" }
                            }
                            button { class: "proof-submit-btn", "+ Submit Proof" }
                        }
                        div {
                            class: "proofs-list",
                            for proof in proofs.iter() {
                                ProofCard {
                                    proof: proof.clone(),
                                    on_assign: on_open_token_picker.clone(),
                                }
                            }
                        }
                    }
                }
            }

            // Token picker modal
            if token_picker_open {
                TokenPickerModal {
                    items: attention_items.clone(),
                    on_close: on_close_token_picker.clone(),
                }
            }
        }
    }
}
