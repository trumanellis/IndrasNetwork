//! Recovery Use overlay — assemble shares collected from backup friends
//! and unlock the local keystore.
//!
//! Phase 1 debug-grade UI: the user pastes each friend's encrypted
//! share alongside the matching steward keypair (decap + encap hex).
//! The bridge decrypts each share, recombines the Shamir shares into
//! the encryption subkey, and re-authenticates the on-disk
//! `StoryKeystore` against the persisted verification token.
//!
//! The "fake friend" button on the Backup-plan overlay produces dk and
//! ek hex; mirror those back here to practice recovery without needing
//! actual steward cooperation.

use dioxus::prelude::*;

use crate::recovery_bridge::{self, RecoveryContribution};
use crate::state::AppState;

#[derive(Clone, Debug, Default)]
struct ContributionRow {
    label: String,
    share_hex: String,
    decap_hex: String,
    encap_hex: String,
}

impl From<&ContributionRow> for RecoveryContribution {
    fn from(row: &ContributionRow) -> Self {
        RecoveryContribution {
            share_hex: row.share_hex.trim().to_string(),
            decap_key_hex: row.decap_hex.trim().to_string(),
            encap_key_hex: row.encap_hex.trim().to_string(),
        }
    }
}

fn row_ready(row: &ContributionRow) -> bool {
    !row.share_hex.trim().is_empty()
        && !row.decap_hex.trim().is_empty()
        && !row.encap_hex.trim().is_empty()
}

/// Recovery-side overlay. Opened via `state.show_recovery_use = true`.
#[component]
pub fn RecoveryUseOverlay(mut state: Signal<AppState>) -> Element {
    if !state.read().show_recovery_use {
        return rsx! {};
    }

    let mut threshold = use_signal(|| 3u8);
    let mut rows = use_signal(|| vec![ContributionRow::default(); 3]);
    let mut status = use_signal(|| None::<(String, bool)>);
    let mut busy = use_signal(|| false);
    let mut succeeded = use_signal(|| false);

    let total_rows = rows.read().len();
    let k_value = *threshold.read();
    let filled = rows.read().iter().filter(|r| row_ready(r)).count();
    let ready = !*busy.read() && !*succeeded.read() && filled >= k_value as usize && k_value >= 2;

    let close = move |_| {
        state.write().show_recovery_use = false;
    };

    rsx! {
        div { class: "recovery-overlay",
            div { class: "recovery-modal",
                header { class: "recovery-header",
                    div { class: "recovery-eyebrow", "IF YOU LOST ACCESS" }
                    div { class: "recovery-title", "Use my backup" }
                    button { class: "recovery-close", onclick: close, "✕" }
                }

                div { class: "recovery-body",
                    div { class: "recovery-intro",
                        "Lost your device or forgot your story? Ask your "
                        b { "backup friends" }
                        " to send back the pieces they're keeping for you. "
                        "Paste each piece here. Once you have enough, you're back in."
                    }

                    // ── 01 · Pieces ───────────────────────────────────
                    section { class: "recovery-section",
                        div { class: "recovery-section-num", "01 · Your pieces · {filled} / {total_rows}" }
                        div { class: "recovery-section-hint",
                            "Each friend has a piece, a secret, and a code. "
                            "Paste all three from each friend into one card below."
                        }

                        for (i, row) in rows.read().clone().into_iter().enumerate() {
                            div {
                                key: "{i}",
                                class: "recovery-steward-card",
                                div { class: "recovery-steward-head",
                                    span { class: "recovery-steward-num", "FRIEND · {i + 1}" }
                                    button {
                                        class: "recovery-steward-x",
                                        title: "Remove this piece",
                                        disabled: total_rows <= 2 || *succeeded.read(),
                                        onclick: move |_| {
                                            let mut current = rows.read().clone();
                                            current.remove(i);
                                            rows.set(current);
                                        },
                                        "✕"
                                    }
                                }
                                div { class: "recovery-field",
                                    label { class: "recovery-field-label", "WHICH FRIEND (OPTIONAL)" }
                                    input {
                                        class: "recovery-input",
                                        placeholder: "e.g. Alex",
                                        value: "{row.label}",
                                        disabled: *succeeded.read(),
                                        oninput: move |evt| {
                                            rows.write()[i].label = evt.value();
                                        },
                                    }
                                }
                                div { class: "recovery-field",
                                    label { class: "recovery-field-label", "THEIR PIECE" }
                                    textarea {
                                        class: "recovery-textarea recovery-mono",
                                        rows: "3",
                                        placeholder: "Paste the long puzzle piece they sent you",
                                        value: "{row.share_hex}",
                                        disabled: *succeeded.read(),
                                        oninput: move |evt| {
                                            rows.write()[i].share_hex = evt.value();
                                        },
                                    }
                                }
                                div { class: "recovery-field",
                                    label { class: "recovery-field-label", "THEIR SECRET" }
                                    textarea {
                                        class: "recovery-textarea recovery-mono",
                                        rows: "2",
                                        placeholder: "Paste the secret half of their key",
                                        value: "{row.decap_hex}",
                                        disabled: *succeeded.read(),
                                        oninput: move |evt| {
                                            rows.write()[i].decap_hex = evt.value();
                                        },
                                    }
                                }
                                div { class: "recovery-field",
                                    label { class: "recovery-field-label", "THEIR CODE" }
                                    textarea {
                                        class: "recovery-textarea recovery-mono",
                                        rows: "2",
                                        placeholder: "Paste the public half of their key",
                                        value: "{row.encap_hex}",
                                        disabled: *succeeded.read(),
                                        oninput: move |evt| {
                                            rows.write()[i].encap_hex = evt.value();
                                        },
                                    }
                                }
                            }
                        }
                        button {
                            class: "se-btn-outline recovery-add",
                            disabled: *succeeded.read(),
                            onclick: move |_| {
                                let mut current = rows.read().clone();
                                current.push(ContributionRow::default());
                                rows.set(current);
                            },
                            "+ Add another piece"
                        }
                    }

                    // ── 02 · How many pieces are needed ───────────────
                    section { class: "recovery-section",
                        div { class: "recovery-section-num", "02 · How many pieces are needed" }
                        div { class: "recovery-section-hint",
                            "Match this to the number your original backup required. "
                            "If you don't remember, start with 3 and adjust if it doesn't work."
                        }
                        div { class: "recovery-stepper",
                            button {
                                class: "recovery-step-btn",
                                disabled: k_value <= 2 || *succeeded.read(),
                                onclick: move |_| {
                                    let cur = *threshold.read();
                                    if cur > 2 { threshold.set(cur - 1); }
                                },
                                "−"
                            }
                            span { class: "recovery-step-value", "{k_value} of {total_rows}" }
                            button {
                                class: "recovery-step-btn",
                                disabled: (k_value as usize) >= total_rows || *succeeded.read(),
                                onclick: move |_| {
                                    let cur = *threshold.read();
                                    if (cur as usize) < total_rows { threshold.set(cur + 1); }
                                },
                                "+"
                            }
                        }
                    }

                    if let Some((ref s, is_err)) = *status.read() {
                        div {
                            class: if is_err { "recovery-status recovery-status-error" } else { "recovery-status" },
                            "{s}"
                        }
                    }
                }

                footer { class: "recovery-footer",
                    if !*succeeded.read() {
                        button {
                            class: "se-btn-glow",
                            disabled: !ready,
                            onclick: move |_| {
                                let contributions: Vec<RecoveryContribution> = rows
                                    .read()
                                    .iter()
                                    .filter(|r| row_ready(r))
                                    .map(RecoveryContribution::from)
                                    .collect();
                                let k = *threshold.read();

                                busy.set(true);
                                status.set(Some(("Piecing your backup together...".to_string(), false)));

                                spawn(async move {
                                    match recovery_bridge::use_steward_recovery(contributions, k).await {
                                        Ok(_) => {
                                            succeeded.set(true);
                                            status.set(Some((
                                                "You're back in. Your identity is unlocked on this device.".to_string(),
                                                false,
                                            )));
                                        }
                                        Err(e) => {
                                            status.set(Some((e, true)));
                                        }
                                    }
                                    busy.set(false);
                                });
                            },
                            if *busy.read() { "Working..." } else { "Unlock with these pieces" }
                        }
                    } else {
                        button {
                            class: "se-btn-glow",
                            onclick: move |_| {
                                state.write().show_recovery_use = false;
                            },
                            "Done"
                        }
                    }
                }
            }
        }
    }
}
