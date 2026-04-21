//! Recovery Setup overlay — nominate K-of-N stewards for the local keystore.
//!
//! Phase 1 debug-grade UI. The user confirms (or re-enters) their 23-slot
//! pass story and pastes each steward's ML-KEM-768 encapsulation key as
//! hex (or generates a test keypair inline). The bridge re-derives the
//! encryption subkey, splits it Shamir K-of-N, encrypts each share to
//! the matching steward, persists a local manifest, and returns one
//! hex-encoded encrypted share per steward for out-of-band delivery.
//!
//! Section 01 reuses the hero's-journey manuscript chrome from
//! `pass_story.rs` and is collapsed by default — when the user is
//! already authenticated their slots are pre-populated from
//! `AppState::pass_story_slots`.

use std::sync::Arc;

use dioxus::prelude::*;

use indras_crypto::StoryTemplate;
use indras_network::IndrasNetwork;

use crate::recovery_bridge::{self, AvailableSteward, StewardInput};
use crate::state::AppState;

/// Per-slot placeholder hints (mirrors `pass_story::SLOT_HINTS`).
const SLOT_HINTS: [&str; 23] = [
    "a land or place you knew",
    "a name you were called",
    "a messenger or force",
    "what they carried",
    "a bond that held you",
    "a shadow that followed",
    "a gate or passage",
    "an unknown realm",
    "a guide who appeared",
    "a hidden truth",
    "something you forged",
    "a raw material",
    "another element",
    "something precious, broken",
    "an opposing force",
    "what rose from silence",
    "what it whispered of",
    "what you carried home",
    "a vast wilderness",
    "your former self",
    "who you became",
    "a gift you keep",
    "another boon you hold",
];

#[derive(Clone, Debug, Default)]
struct StewardRow {
    label: String,
    ek_hex: String,
    /// Decapsulation key captured when the user clicks "Try with a fake
    /// friend". Shown inline on the card so they can copy it for later
    /// practice recovery; `None` when the friend is a real peer.
    test_decap: Option<String>,
    /// Peer `UserId` hex when this row was added via the peer picker.
    /// Drives in-band share delivery — empty rows fall back to the hex
    /// copy-paste path.
    user_id_hex: Option<String>,
}

/// Recovery Setup overlay. Opened via `state.show_recovery_setup = true`.
#[component]
pub fn RecoverySetupOverlay(
    mut state: Signal<AppState>,
    network: Signal<Option<Arc<IndrasNetwork>>>,
) -> Element {
    if !state.read().show_recovery_setup {
        return rsx! {};
    }

    // Seed from the in-session slots if available (signed-in users have these).
    let initial_slots: Vec<String> = {
        let stored = state.read().pass_story_slots.clone();
        if stored.len() == 23 { stored } else { vec![String::new(); 23] }
    };

    let slots = use_signal(move || initial_slots);
    let mut story_open = use_signal(|| false);
    let mut threshold = use_signal(|| 3u8);
    let mut rows = use_signal(|| vec![StewardRow::default(); 3]);
    let mut status = use_signal(|| None::<(String, bool)>); // (msg, is_error)
    let mut busy = use_signal(|| false);
    let mut shares_hex = use_signal(Vec::<String>::new);
    let mut available = use_signal(Vec::<AvailableSteward>::new);

    // Populate the peer picker and refresh held-backup count on open,
    // re-running each time the overlay becomes visible.
    use_effect(move || {
        if let Some(net) = network.read().clone() {
            let net_picker = net.clone();
            spawn(async move {
                let list = recovery_bridge::list_available_stewards(net_picker).await;
                available.set(list);
            });
            let net_holdings = net.clone();
            spawn(async move {
                let holdings = recovery_bridge::refresh_held_backups(net_holdings).await;
                state.write().held_backups_count = holdings.count();
            });
        }
    });

    let total_stewards = rows.read().len();
    let k_value = *threshold.read();
    let filled_slots = slots.read().iter().filter(|s| !s.trim().is_empty()).count();
    let story_complete = filled_slots == 23;
    let story_open_now = *story_open.read();
    let has_cached_key = recovery_bridge::has_cached_subkey();
    // The pass story is only required when we have nothing cached. If
    // the subkey is on disk (populated at sign-in), treat section 01
    // as optional so the backup flow doesn't make the user retype 23
    // words they already spoke.
    let story_satisfied = has_cached_key || story_complete;

    let ready = !*busy.read()
        && story_satisfied
        && total_stewards >= k_value as usize
        && k_value >= 2
        && rows
            .read()
            .iter()
            .all(|r| !r.label.trim().is_empty() && !r.ek_hex.trim().is_empty());

    let close = move |_| {
        state.write().show_recovery_setup = false;
    };

    rsx! {
        div { class: "recovery-overlay",
            div { class: "recovery-modal",
                header { class: "recovery-header",
                    div { class: "recovery-eyebrow", "IF YOU LOSE ACCESS" }
                    div { class: "recovery-title", "Set up a backup plan" }
                    button { class: "recovery-close", onclick: close, "✕" }
                }

                div { class: "recovery-body",

                    // ── Opening explainer ─────────────────────────────
                    div { class: "recovery-intro",
                        "If you ever lose your password or your phone, your "
                        b { "backup friends" }
                        " can help you get back in. "
                        "Pick a few people you trust. Each one gets a puzzle piece. "
                        "Any few of them, working together, can help you recover."
                    }

                    // ── 01 · Your story (collapsible) ─────────────────
                    section { class: "recovery-section",
                        button {
                            class: "recovery-disclosure",
                            onclick: move |_| {
                                let cur = *story_open.read();
                                story_open.set(!cur);
                            },
                            span { class: "recovery-disclosure-arrow",
                                if story_open_now { "▾" } else { "▸" }
                            }
                            span { class: "recovery-section-num",
                                if has_cached_key {
                                    "01 · Your story (optional)"
                                } else {
                                    "01 · Your story"
                                }
                            }
                            span { class: "recovery-disclosure-meta",
                                if has_cached_key && !story_complete {
                                    "already saved"
                                } else if story_complete {
                                    "23 / 23 ✓"
                                } else {
                                    "{filled_slots} / 23"
                                }
                            }
                        }

                        if story_open_now {
                            div { class: "recovery-section-hint",
                                "Type the secret words from when you first made your account. "
                                "This proves it's really you before we build the backup. "
                                "The words stay on this device."
                            }
                            div { class: "recovery-manuscript",
                                {render_story_manuscript(slots)}
                            }
                        }
                    }

                    // ── 02 · Backup friends ───────────────────────────
                    section { class: "recovery-section",
                        div { class: "recovery-section-num", "02 · Backup friends · {total_stewards}" }
                        div { class: "recovery-section-hint",
                            "Pick people who can each keep one piece of your backup. "
                            "More friends is safer. You can change this later."
                        }

                        // Peer picker — peers known to the network who've published a backup code.
                        if !available.read().is_empty() {
                            div { class: "recovery-picker",
                                div { class: "recovery-picker-label", "FROM YOUR PEERS" }
                                div { class: "recovery-picker-list",
                                    for (j, peer) in available.read().clone().into_iter().enumerate() {
                                        button {
                                            key: "{j}",
                                            class: "recovery-picker-item",
                                            title: "Add this peer as a backup friend",
                                            onclick: move |_| {
                                                let empty = rows.read().iter().position(|r| {
                                                    r.label.trim().is_empty() && r.ek_hex.trim().is_empty()
                                                });
                                                let target = match empty {
                                                    Some(i) => i,
                                                    None => {
                                                        let mut cur = rows.read().clone();
                                                        cur.push(StewardRow::default());
                                                        rows.set(cur);
                                                        rows.read().len() - 1
                                                    }
                                                };
                                                rows.write()[target].label = peer.label.clone();
                                                rows.write()[target].ek_hex = peer.ek_hex.clone();
                                                rows.write()[target].test_decap = None;
                                                rows.write()[target].user_id_hex = Some(peer.user_id_hex.clone());
                                                status.set(Some((
                                                    format!("Added {} as a backup friend.", peer.label),
                                                    false,
                                                )));
                                            },
                                            span { class: "recovery-picker-dot" }
                                            span { class: "recovery-picker-name", "{peer.label}" }
                                            span { class: "recovery-picker-add", "+" }
                                        }
                                    }
                                }
                            }
                        }

                        for (i, row) in rows.read().clone().into_iter().enumerate() {
                            div {
                                key: "{i}",
                                class: "recovery-steward-card",
                                div { class: "recovery-steward-head",
                                    span { class: "recovery-steward-num", "FRIEND · {i + 1}" }
                                    button {
                                        class: "recovery-steward-x",
                                        title: "Remove this friend",
                                        disabled: total_stewards <= 2,
                                        onclick: move |_| {
                                            let mut current = rows.read().clone();
                                            current.remove(i);
                                            rows.set(current);
                                        },
                                        "✕"
                                    }
                                }
                                div { class: "recovery-field",
                                    label { class: "recovery-field-label", "THEIR NAME" }
                                    input {
                                        class: "recovery-input",
                                        placeholder: "e.g. Alex",
                                        value: "{row.label}",
                                        oninput: move |evt| {
                                            rows.write()[i].label = evt.value();
                                        },
                                    }
                                }
                                div { class: "recovery-field",
                                    label { class: "recovery-field-label", "THEIR RECOVERY CODE" }
                                    textarea {
                                        class: "recovery-textarea recovery-mono",
                                        rows: "2",
                                        placeholder: "Ask them for their backup code — it's a long string of letters and numbers",
                                        value: "{row.ek_hex}",
                                        oninput: move |evt| {
                                            rows.write()[i].ek_hex = evt.value();
                                        },
                                    }
                                }
                                if let Some(dk) = row.test_decap.clone() {
                                    div { class: "recovery-field recovery-field-test",
                                        label { class: "recovery-field-label",
                                            "FAKE FRIEND'S SECRET · SAVE TO PRACTICE RECOVERY"
                                        }
                                        textarea {
                                            class: "recovery-textarea recovery-mono recovery-test-decap",
                                            rows: "3",
                                            readonly: true,
                                            value: "{dk}",
                                        }
                                    }
                                }
                                div { class: "recovery-steward-foot",
                                    button {
                                        class: "se-btn-text",
                                        title: "Pretend this friend already gave you their code (for testing)",
                                        onclick: move |_| {
                                            let (dk, ek) = recovery_bridge::generate_test_steward_keypair();
                                            let label = if rows.read()[i].label.trim().is_empty() {
                                                format!("test-friend-{}", i + 1)
                                            } else {
                                                rows.read()[i].label.clone()
                                            };
                                            rows.write()[i].label = label;
                                            rows.write()[i].ek_hex = ek;
                                            rows.write()[i].test_decap = Some(dk);
                                            rows.write()[i].user_id_hex = None;
                                            status.set(Some((
                                                format!(
                                                    "Fake friend #{} ready. Their secret is shown on the card — copy it if you want to practice recovery later.",
                                                    i + 1,
                                                ),
                                                false,
                                            )));
                                        },
                                        "Try with a fake friend"
                                    }
                                }
                            }
                        }
                        button {
                            class: "se-btn-outline recovery-add",
                            onclick: move |_| {
                                let mut current = rows.read().clone();
                                current.push(StewardRow::default());
                                rows.set(current);
                            },
                            "+ Add a friend"
                        }
                    }

                    // ── 03 · How many friends must help ───────────────
                    section { class: "recovery-section",
                        div { class: "recovery-section-num", "03 · How many friends must help" }
                        div { class: "recovery-section-hint",
                            "If you lose access, you'll ask your friends for help. "
                            "This many need to cooperate before you get back in. "
                            "Higher = safer. Lower = easier to recover."
                        }
                        div { class: "recovery-stepper",
                            button {
                                class: "recovery-step-btn",
                                disabled: k_value <= 2,
                                onclick: move |_| {
                                    let cur = *threshold.read();
                                    if cur > 2 { threshold.set(cur - 1); }
                                },
                                "−"
                            }
                            span { class: "recovery-step-value", "{k_value} of {total_stewards}" }
                            button {
                                class: "recovery-step-btn",
                                disabled: (k_value as usize) >= total_stewards,
                                onclick: move |_| {
                                    let cur = *threshold.read();
                                    if (cur as usize) < total_stewards { threshold.set(cur + 1); }
                                },
                                "+"
                            }
                        }
                    }

                    // ── Status ────────────────────────────────────────
                    if let Some((ref s, is_err)) = *status.read() {
                        div {
                            class: if is_err { "recovery-status recovery-status-error" } else { "recovery-status" },
                            "{s}"
                        }
                    }

                    // ── Output: puzzle pieces ─────────────────────────
                    if !shares_hex.read().is_empty() {
                        section { class: "recovery-section",
                            div { class: "recovery-section-num", "04 · Send these to your friends" }
                            div { class: "recovery-section-hint",
                                "Each friend gets one piece. Send it privately — a trusted app, "
                                "a USB stick, or in person. They'll need this piece if you ever ask "
                                "them to help you recover."
                            }
                            for (i, share) in shares_hex.read().clone().into_iter().enumerate() {
                                div {
                                    key: "{i}",
                                    class: "recovery-share-block",
                                    div { class: "recovery-share-label", "PIECE FOR FRIEND · {i + 1}" }
                                    textarea {
                                        class: "recovery-textarea recovery-mono recovery-share-output",
                                        rows: "4",
                                        readonly: true,
                                        value: "{share}",
                                    }
                                }
                            }
                        }
                    }
                }

                footer { class: "recovery-footer",
                    if !story_satisfied {
                        span { class: "recovery-footer-hint",
                            "Open Your story above and fill it in to continue"
                        }
                    }
                    button {
                        class: "se-btn-glow",
                        disabled: !ready,
                        onclick: move |_| {
                            let story_slots = slots.read().clone();
                            let k = *threshold.read();
                            let steward_input: Vec<StewardInput> = rows
                                .read()
                                .iter()
                                .map(|r| StewardInput {
                                    label: r.label.trim().to_string(),
                                    ek_hex: r.ek_hex.trim().to_string(),
                                    user_id_hex: r.user_id_hex.clone(),
                                })
                                .collect();
                            let net = network.read().clone();

                            busy.set(true);
                            status.set(Some(("Making your puzzle pieces...".to_string(), false)));
                            shares_hex.set(Vec::new());

                            spawn(async move {
                                match recovery_bridge::setup_steward_recovery(story_slots, steward_input, k, net).await {
                                    Ok(outcome) => {
                                        let total = outcome.shares_hex.len();
                                        let delivered = outcome.delivered_to.len();
                                        let msg = if delivered == 0 {
                                            format!(
                                                "Done. You have {} pieces — send each one to the matching friend.",
                                                total
                                            )
                                        } else if delivered == total {
                                            format!(
                                                "Done. All {} pieces were sent to your friends automatically.",
                                                total
                                            )
                                        } else {
                                            format!(
                                                "Done. {} of {} pieces were sent automatically; the rest are below — copy each to the matching friend.",
                                                delivered, total
                                            )
                                        };
                                        status.set(Some((msg, false)));
                                        shares_hex.set(outcome.shares_hex);
                                    }
                                    Err(e) => {
                                        status.set(Some((format!("Something went wrong: {}", e), true)));
                                    }
                                }
                                busy.set(false);
                            });
                        },
                        if *busy.read() { "Working..." } else { "Make my backup" }
                    }
                }
            }
        }
    }
}

/// Render the 23 slots inside the hero's-journey template (manuscript style).
fn render_story_manuscript(mut slots: Signal<Vec<String>>) -> Element {
    let template = StoryTemplate::default_template();
    let mut global_slot = 0usize;

    rsx! {
        for (stage_idx, stage) in template.stages.iter().enumerate() {
            {
                let parts: Vec<String> = stage
                    .template
                    .split("`_____`")
                    .map(|s| s.to_string())
                    .collect();
                let slot_count = stage.slot_count;
                let start_slot = global_slot;
                global_slot += slot_count;
                let stage_name = stage.name;

                rsx! {
                    div {
                        key: "{stage_idx}",
                        class: "story-manuscript-stage",
                        span { class: "story-stage-annotation", "{stage_name}" }
                        p { class: "story-manuscript-paragraph",
                            for (i, part) in parts.iter().enumerate() {
                                span { class: "story-prose", "{part}" }
                                if i < slot_count {
                                    {
                                        let slot_idx = start_slot + i;
                                        let hint = SLOT_HINTS.get(slot_idx).copied().unwrap_or("...");
                                        let current_val = slots.read().get(slot_idx).cloned().unwrap_or_default();
                                        rsx! {
                                            input {
                                                class: "story-manuscript-blank",
                                                r#type: "text",
                                                value: "{current_val}",
                                                placeholder: "{hint}",
                                                oninput: move |evt| {
                                                    if let Some(slot) = slots.write().get_mut(slot_idx) {
                                                        *slot = evt.value();
                                                    }
                                                },
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
    }
}
