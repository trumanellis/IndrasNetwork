//! Quest/Need/Offering/Intention view component — full Intention Game Loop.

use dioxus::prelude::*;
use indras_ui::{ArtifactDisplayInfo, ArtifactGallery};

// ================================================================
// Data Types
// ================================================================

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

/// A pledged token on this intention.
#[derive(Clone, Debug, PartialEq)]
pub struct PledgedToken {
    pub token_label: String,
    pub duration: String,
    pub from_name: String,
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

/// Per-peer attention summary for the attention section.
#[derive(Clone, Debug, PartialEq)]
pub struct AttentionPeerSummary {
    pub peer_name: String,
    pub peer_letter: String,
    pub peer_color_class: String,
    pub total_duration: String,
    pub total_duration_secs: u64,
    pub window_count: usize,
    pub bar_fraction: f32,
}

/// A single link in the stewardship chain visualization.
#[derive(Clone, Debug, PartialEq)]
pub struct StewardshipChainEntry {
    pub from_name: String,
    pub from_letter: String,
    pub from_color_class: String,
    pub action: String,
    pub token_label: String,
    pub token_duration: String,
    pub to_name: String,
    pub to_letter: String,
    pub to_color_class: String,
}

// ================================================================
// Utility Functions
// ================================================================

/// Parse a duration string like "6m 33s" or "12m 08s" to seconds.
fn parse_duration_to_secs(s: &str) -> u64 {
    let mut total = 0u64;
    let s = s.trim();
    for part in s.split_whitespace() {
        if let Some(m) = part.strip_suffix('m') {
            total += m.parse::<u64>().unwrap_or(0) * 60;
        } else if let Some(s) = part.strip_suffix('s') {
            total += s.parse::<u64>().unwrap_or(0);
        }
    }
    total
}

/// Format seconds as "Xm Ys".
pub fn format_duration_secs(secs: u64) -> String {
    let m = secs / 60;
    let s = secs % 60;
    format!("{m}m {s:02}s")
}

/// Map a heat float to a CSS class name.
fn heat_class(heat: f32) -> &'static str {
    if heat >= 0.8 { "heat-5" }
    else if heat >= 0.6 { "heat-4" }
    else if heat >= 0.4 { "heat-3" }
    else if heat >= 0.2 { "heat-2" }
    else if heat > 0.0 { "heat-1" }
    else { "heat-0" }
}

// ================================================================
// Sub-Components
// ================================================================

/// Attention summary — collective and per-peer breakdown with heat bar.
#[component]
fn AttentionSummary(
    entries: Vec<AttentionPeerSummary>,
    total_duration: String,
    heat: f32,
) -> Element {
    let heat_pct = (heat * 100.0).min(100.0) as u32;
    let hclass = heat_class(heat);

    rsx! {
        div {
            class: "attention-summary",
            div {
                class: "attention-summary-header",
                div { class: "attention-summary-title", "Collective Attention" }
                div {
                    class: "attention-summary-total",
                    "{total_duration} total"
                }
                div {
                    class: "attention-heat",
                    span { class: "attention-heat-label", "Heat" }
                    div {
                        class: "attention-heat-track",
                        div {
                            class: "attention-heat-fill {hclass}",
                            style: "width:{heat_pct}%",
                        }
                    }
                }
            }
            for peer in entries.iter() {
                {
                    let bar_pct = (peer.bar_fraction * 100.0) as u32;
                    rsx! {
                        div {
                            class: "attention-peer-row",
                            div {
                                class: "proof-card-avatar {peer.peer_color_class}",
                                "{peer.peer_letter}"
                            }
                            div { class: "attention-peer-name", "{peer.peer_name}" }
                            div {
                                class: "attention-bar-track",
                                div {
                                    class: "attention-bar-fill",
                                    style: "width:{bar_pct}%",
                                }
                            }
                            div {
                                class: "attention-peer-duration",
                                "{peer.total_duration}"
                            }
                            div {
                                class: "attention-peer-windows",
                                "({peer.window_count} windows)"
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Inline proof submission — textarea + submit button.
#[component]
fn ProofSubmitInline(
    on_submit: EventHandler<String>,
) -> Element {
    let mut draft = use_signal(String::new);
    let is_empty = draft.read().trim().is_empty();

    rsx! {
        div {
            class: "proof-submit-inline",
            textarea {
                class: "proof-submit-textarea",
                placeholder: "What did you do? Describe your proof of service...",
                value: "{draft}",
                oninput: move |e| draft.set(e.value()),
            }
            div {
                class: "proof-submit-actions",
                button {
                    class: "proof-submit-confirm",
                    disabled: is_empty,
                    onclick: {
                        let handler = on_submit.clone();
                        move |_| {
                            let body = draft.read().clone();
                            if !body.trim().is_empty() {
                                handler.call(body);
                                draft.set(String::new());
                            }
                        }
                    },
                    "Submit Proof"
                }
            }
        }
    }
}

/// Inline token picker — renders within proof card (not as modal overlay).
#[component]
fn InlineTokenPicker(
    items: Vec<AttentionItem>,
    on_confirm: EventHandler<Vec<usize>>,
    on_close: EventHandler<()>,
) -> Element {
    let mut selected = use_signal(|| std::collections::HashSet::<usize>::new());

    let selected_count = selected.read().len();
    let has_selection = selected_count > 0;

    let total_label = {
        let sel = selected.read();
        let total_secs: u64 = items.iter().enumerate()
            .filter(|(i, _)| sel.contains(i))
            .map(|(_, item)| parse_duration_to_secs(&item.duration))
            .sum();
        format_duration_secs(total_secs)
    };

    rsx! {
        div {
            class: "inline-picker",
            div {
                class: "inline-picker-header",
                div { class: "inline-picker-title", "Assign Attention" }
                button {
                    class: "inline-picker-close",
                    onclick: move |_| on_close.call(()),
                    "\u{2715}"
                }
            }
            div {
                class: "inline-picker-subtitle",
                "Select attention artifacts to assign as tokens. Dwell time becomes the token\u{2019}s value."
            }
            div {
                class: "inline-picker-list",
                for (idx, item) in items.iter().enumerate() {
                    {
                        let is_selected = selected.read().contains(&idx);
                        let row_class = if is_selected { "attn-row selected" } else { "attn-row" };
                        rsx! {
                            div {
                                class: "{row_class}",
                                onclick: move |_| {
                                    let mut sel = selected.write();
                                    if sel.contains(&idx) {
                                        sel.remove(&idx);
                                    } else {
                                        sel.insert(idx);
                                    }
                                },
                                div {
                                    class: "attn-check",
                                    if is_selected { "\u{2713}" } else { "" }
                                }
                                div {
                                    class: "attn-info",
                                    div { class: "attn-target", "{item.target}" }
                                    div { class: "attn-when", "{item.when}" }
                                }
                                div { class: "attn-duration", "{item.duration}" }
                            }
                        }
                    }
                }
            }
            div {
                class: "inline-picker-footer",
                div {
                    class: "picker-total",
                    strong { "{selected_count}" }
                    " selected \u{00B7} "
                    strong { "{total_label}" }
                }
                button {
                    class: "picker-confirm",
                    disabled: !has_selection,
                    onclick: {
                        let handler = on_confirm.clone();
                        move |_| {
                            let indices: Vec<usize> = selected.read().iter().copied().collect();
                            handler.call(indices);
                        }
                    },
                    "Assign Tokens"
                }
            }
        }
    }
}

/// Individual proof card component with optional inline token picker.
#[component]
fn ProofCard(
    proof: ProofEntry,
    proof_idx: usize,
    #[props(default = false)]
    picker_open: bool,
    #[props(default = Vec::new())]
    attention_items: Vec<AttentionItem>,
    on_toggle_picker: Option<EventHandler<usize>>,
    on_confirm_tokens: Option<EventHandler<(usize, Vec<usize>)>>,
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
            // Artifact attachments
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
            // Token assignment section
            div {
                class: "proof-tokens",
                div {
                    class: "proof-tokens-header",
                    div {
                        class: "proof-tokens-title",
                        span { class: "token-icon", "\u{2728}" }
                        " Tokens of Gratitude"
                    }
                    if let Some(handler) = on_toggle_picker {
                        {
                            let idx = proof_idx;
                            rsx! {
                                button {
                                    class: "assign-btn",
                                    onclick: move |_| handler.call(idx),
                                    "{assign_label}"
                                }
                            }
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
            // Inline token picker (when open for this proof)
            if picker_open {
                {
                    let idx = proof_idx;
                    let confirm_handler = on_confirm_tokens;
                    let close_handler = on_toggle_picker;
                    rsx! {
                        InlineTokenPicker {
                            items: attention_items.clone(),
                            on_confirm: move |indices: Vec<usize>| {
                                if let Some(h) = confirm_handler {
                                    h.call((idx, indices));
                                }
                            },
                            on_close: move |_| {
                                if let Some(h) = close_handler {
                                    h.call(idx);
                                }
                            },
                        }
                    }
                }
            }
        }
    }
}

/// Pledged tokens section with release actions.
#[component]
fn PledgedTokensSection(
    tokens: Vec<PledgedToken>,
    on_release: Option<EventHandler<Vec<usize>>>,
) -> Element {
    let count = tokens.len();

    rsx! {
        div {
            class: "pledged-tokens-section",
            div {
                class: "proofs-header",
                div {
                    class: "proofs-title",
                    "Pledged Tokens "
                    span { class: "proofs-count", "{count}" }
                }
                if let Some(handler) = on_release {
                    button {
                        class: "assign-btn",
                        onclick: move |_| {
                            let all: Vec<usize> = (0..count).collect();
                            handler.call(all);
                        },
                        "Release All"
                    }
                }
            }
            div {
                class: "pledged-tokens-list",
                for pt in tokens.iter() {
                    div {
                        class: "assigned-token",
                        span { class: "token-time-pill", "{pt.duration}" }
                        span { class: "token-source", "from {pt.from_name}" }
                        span { class: "token-label", "{pt.token_label}" }
                    }
                }
            }
        }
    }
}

/// Stewardship chain visualization.
#[component]
fn StewardshipChain(
    entries: Vec<StewardshipChainEntry>,
) -> Element {
    rsx! {
        div {
            class: "stewardship-chain",
            div {
                class: "proofs-header",
                div {
                    class: "proofs-title",
                    "Stewardship Chain"
                }
            }
            div {
                class: "chain-entries",
                for entry in entries.iter() {
                    div {
                        class: "chain-entry",
                        div {
                            class: "chain-avatar {entry.from_color_class}",
                            "{entry.from_letter}"
                        }
                        span { class: "chain-from", "{entry.from_name}" }
                        span { class: "chain-arrow", "\u{2192}" }
                        span { class: "chain-action {entry.action}", "{entry.action}" }
                        span { class: "chain-arrow", "\u{2192}" }
                        span { class: "chain-token", "{entry.token_duration}" }
                        span { class: "chain-arrow", "\u{2192}" }
                        div {
                            class: "chain-avatar {entry.to_color_class}",
                            "{entry.to_letter}"
                        }
                        span { class: "chain-to", "{entry.to_name}" }
                    }
                }
            }
        }
    }
}

// ================================================================
// Main QuestView Component
// ================================================================

/// Quest view — hero card + attention summary + proofs + pledges + stewardship chain.
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
    #[props(default = 0.0)]
    heat: f32,
    #[props(default = Vec::new())]
    attention_peers: Vec<AttentionPeerSummary>,
    #[props(default = String::new())]
    total_attention_duration: String,
    #[props(default = Vec::new())]
    attention_items: Vec<AttentionItem>,
    on_submit_proof: Option<EventHandler<String>>,
    on_confirm_tokens: Option<EventHandler<(usize, Vec<usize>)>>,
    #[props(default = Vec::new())]
    pledged_tokens: Vec<PledgedToken>,
    #[props(default = Vec::new())]
    stewardship_chain: Vec<StewardshipChainEntry>,
    on_release_pledged: Option<EventHandler<Vec<usize>>>,
) -> Element {
    let mut active_picker_proof_idx = use_signal(|| None::<usize>);

    let card_class = format!("quest-card {}", kind.css_class());
    let status_class = format!("quest-status {}", status.to_lowercase());
    let audience_text = format!("{} audience", audience_count);
    let proofs_count = proofs.len();
    let total_tokens: usize = proofs.iter().map(|p| p.total_token_count).sum();
    let token_meta = if total_tokens > 0 {
        format!("\u{2728} {} tokens assigned", total_tokens)
    } else {
        String::new()
    };
    let heat_pct = (heat * 100.0).min(100.0) as u32;
    let hclass = heat_class(heat);

    rsx! {
        div {
            class: "view active",
            div {
                class: "request-scroll",
                div {
                    class: "request-layout",

                    // 1. Quest Hero Card
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
                            if heat > 0.0 {
                                div {
                                    class: "quest-meta-item quest-heat",
                                    span { "\u{1F525}" }
                                    div {
                                        class: "quest-heat-track",
                                        div {
                                            class: "quest-heat-fill {hclass}",
                                            style: "width:{heat_pct}%",
                                        }
                                    }
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

                    // 2. Attention Summary
                    if !attention_peers.is_empty() {
                        AttentionSummary {
                            entries: attention_peers.clone(),
                            total_duration: total_attention_duration.clone(),
                            heat: heat,
                        }
                    }

                    // 3. Proofs of Service
                    div {
                        class: "proofs-header",
                        div {
                            class: "proofs-title",
                            "Proofs of Service "
                            span { class: "proofs-count", "{proofs_count}" }
                        }
                    }
                    if !proofs.is_empty() {
                        div {
                            class: "proofs-list",
                            for (idx, proof) in proofs.iter().enumerate() {
                                {
                                    let is_picker_open = *active_picker_proof_idx.read() == Some(idx);
                                    rsx! {
                                        ProofCard {
                                            proof: proof.clone(),
                                            proof_idx: idx,
                                            picker_open: is_picker_open,
                                            attention_items: attention_items.clone(),
                                            on_toggle_picker: move |clicked_idx: usize| {
                                                let current = *active_picker_proof_idx.read();
                                                if current == Some(clicked_idx) {
                                                    active_picker_proof_idx.set(None);
                                                } else {
                                                    active_picker_proof_idx.set(Some(clicked_idx));
                                                }
                                            },
                                            on_confirm_tokens: move |data: (usize, Vec<usize>)| {
                                                active_picker_proof_idx.set(None);
                                                if let Some(h) = on_confirm_tokens {
                                                    h.call(data);
                                                }
                                            },
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Inline proof submission
                    if let Some(handler) = on_submit_proof {
                        ProofSubmitInline {
                            on_submit: handler,
                        }
                    }

                    // 4. Pledged Tokens
                    if !pledged_tokens.is_empty() {
                        PledgedTokensSection {
                            tokens: pledged_tokens.clone(),
                            on_release: on_release_pledged,
                        }
                    }

                    // 5. Stewardship Chain
                    if !stewardship_chain.is_empty() {
                        StewardshipChain {
                            entries: stewardship_chain.clone(),
                        }
                    }
                }
            }
        }
    }
}

// ================================================================
// Intention Create Overlay
// ================================================================

/// A peer option for the audience checkbox list.
#[derive(Clone, Debug, PartialEq)]
pub struct PeerOption {
    pub player_id: [u8; 32],
    pub name: String,
    pub selected: bool,
}

/// Modal overlay for creating a new Intention with title, description, and audience.
#[component]
pub fn IntentionCreateOverlay(
    visible: bool,
    peers: Vec<PeerOption>,
    on_close: EventHandler<()>,
    on_create: EventHandler<(String, String, Vec<[u8; 32]>)>,
) -> Element {
    let mut title = use_signal(String::new);
    let mut description = use_signal(String::new);
    let mut selected_peers = use_signal(|| {
        // All peers selected by default
        let all: std::collections::HashSet<usize> = (0..peers.len()).collect();
        all
    });

    if !visible {
        return rsx! {};
    }

    let title_empty = title.read().trim().is_empty();

    rsx! {
        div {
            class: "intention-create-overlay",
            onclick: move |_| on_close.call(()),
            div {
                class: "intention-create-dialog",
                onclick: move |e| e.stop_propagation(),
                // Header
                div {
                    class: "intention-create-header",
                    div { class: "intention-create-title", "Create Intention" }
                    button {
                        class: "intention-create-close",
                        onclick: move |_| on_close.call(()),
                        "\u{2715}"
                    }
                }
                // Body
                div {
                    class: "intention-create-body",
                    // Title field
                    div {
                        class: "intention-create-field",
                        label { class: "intention-create-label", "Title" }
                        input {
                            class: "intention-create-input",
                            r#type: "text",
                            placeholder: "What's your intention?",
                            value: "{title}",
                            oninput: move |e| title.set(e.value()),
                        }
                    }
                    // Description field
                    div {
                        class: "intention-create-field",
                        label { class: "intention-create-label", "Description" }
                        textarea {
                            class: "intention-create-textarea",
                            placeholder: "Describe your intention...",
                            value: "{description}",
                            oninput: move |e| description.set(e.value()),
                        }
                    }
                    // Audience checkboxes
                    if !peers.is_empty() {
                        div {
                            class: "intention-create-field",
                            label { class: "intention-create-label", "Audience" }
                            div {
                                class: "intention-create-audience",
                                for (idx, peer) in peers.iter().enumerate() {
                                    {
                                        let is_checked = selected_peers.read().contains(&idx);
                                        let check_icon = if is_checked { "\u{2611}" } else { "\u{2610}" };
                                        rsx! {
                                            div {
                                                class: "intention-create-peer-row",
                                                onclick: move |_| {
                                                    let mut sel = selected_peers.write();
                                                    if sel.contains(&idx) {
                                                        sel.remove(&idx);
                                                    } else {
                                                        sel.insert(idx);
                                                    }
                                                },
                                                span { class: "intention-create-check", "{check_icon}" }
                                                span { class: "intention-create-peer-name", "{peer.name}" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                // Footer actions
                div {
                    class: "intention-create-actions",
                    button {
                        class: "intention-create-cancel",
                        onclick: move |_| on_close.call(()),
                        "Cancel"
                    }
                    button {
                        class: "intention-create-submit",
                        disabled: title_empty,
                        onclick: {
                            let peers_clone = peers.clone();
                            let handler = on_create.clone();
                            move |_| {
                                let t = title.read().clone();
                                let d = description.read().clone();
                                let audience: Vec<[u8; 32]> = selected_peers.read().iter()
                                    .filter_map(|&i| peers_clone.get(i).map(|p| p.player_id))
                                    .collect();
                                handler.call((t, d, audience));
                                // Reset fields
                                title.set(String::new());
                                description.set(String::new());
                            }
                        },
                        "Create Intention"
                    }
                }
            }
        }
    }
}
