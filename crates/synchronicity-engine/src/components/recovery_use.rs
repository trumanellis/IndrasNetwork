//! Recovery overlay — ask your backup friends for help.
//!
//! Plan-A rewrite. No hex, no decapsulation keys, no manual paste.
//! The user picks which friends to ask, taps a button, and the
//! overlay shows live progress as each friend approves. Once
//! enough have approved, the device re-authenticates the keystore
//! using the reassembled subkey.
//!
//! Plan-A covers the same-device case ("I forgot my story but my
//! data dir is intact"). True cross-device recovery arrives with
//! Plan B's AccountRoot; until then the steward auto-releases the
//! backup whose sender matches the DM peer asking for recovery.

use std::sync::Arc;

use dioxus::prelude::*;

use indras_network::IndrasNetwork;

use crate::recovery_bridge::{self, OutgoingEnrollment};
use crate::state::AppState;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
enum RecoveryPhase {
    #[default]
    Picking,
    Waiting,
    Unlocking,
    Done,
}

/// Recovery overlay. Opened via `state.show_recovery_use = true`.
#[component]
pub fn RecoveryUseOverlay(
    mut state: Signal<AppState>,
    network: Signal<Option<Arc<IndrasNetwork>>>,
) -> Element {
    if !state.read().show_recovery_use {
        return rsx! {};
    }

    let mut threshold = use_signal(|| 3u8);
    let mut peers = use_signal(Vec::<OutgoingEnrollment>::new);
    let mut selected = use_signal(std::collections::BTreeSet::<String>::new);
    let mut status = use_signal(|| None::<(String, bool)>);
    let mut phase = use_signal(RecoveryPhase::default);
    let mut released_by = use_signal(Vec::<String>::new);
    let mut busy = use_signal(|| false);

    // Load the peer list when the overlay opens. Recovery uses the
    // same enrollment list as Backup-plan so the user sees familiar
    // faces — those friends they've already asked for help.
    use_effect(move || {
        if let Some(net) = network.read().clone() {
            spawn(async move {
                let list = recovery_bridge::list_outgoing_enrollments(net).await;
                peers.set(list);
            });
        }
    });

    // While we're waiting for approvals, poll every 2s.
    use_effect(move || {
        if *phase.read() != RecoveryPhase::Waiting {
            return;
        }
        if let Some(net) = network.read().clone() {
            spawn(async move {
                loop {
                    let progress = recovery_bridge::poll_recovery_releases(net.clone()).await;
                    released_by.set(progress.released_by.clone());
                    if progress.count() >= *threshold.read() as usize {
                        // Enough releases — move to Unlocking.
                        phase.set(RecoveryPhase::Unlocking);
                        let k = *threshold.read();
                        let net2 = net.clone();
                        match recovery_bridge::assemble_and_authenticate(net2, k).await {
                            Ok(()) => {
                                phase.set(RecoveryPhase::Done);
                                status.set(Some((
                                    "You're back in. Your identity is unlocked.".to_string(),
                                    false,
                                )));
                            }
                            Err(e) => {
                                status.set(Some((e, true)));
                                phase.set(RecoveryPhase::Waiting);
                            }
                        }
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                    if *phase.read() != RecoveryPhase::Waiting {
                        break;
                    }
                }
            });
        }
    });

    let close = move |_| {
        state.write().show_recovery_use = false;
    };

    let peers_snap = peers.read().clone();
    let k_value = *threshold.read();
    let phase_now = phase.read().clone();
    let released_count = released_by.read().len();

    rsx! {
        div { class: "recovery-overlay",
            div { class: "recovery-modal",
                header { class: "recovery-header",
                    div { class: "recovery-eyebrow", "IF YOU LOST ACCESS" }
                    div { class: "recovery-title", "Ask your friends for help" }
                    button { class: "recovery-close", onclick: close, "✕" }
                }

                div { class: "recovery-body",
                    match phase_now {
                        RecoveryPhase::Picking => rsx! {
                            div { class: "recovery-intro",
                                "Pick the friends you already asked to keep your backup pieces. "
                                "They'll each get a notification. Once enough of them verify "
                                "it's really you and approve, you're back in."
                            }

                            section { class: "recovery-section",
                                div { class: "recovery-section-num",
                                    "01 · Friends to ask"
                                }
                                if peers_snap.is_empty() {
                                    div { class: "recovery-empty",
                                        "No direct-message friends yet. Add a contact first."
                                    }
                                } else {
                                    div { class: "recovery-friend-list",
                                        for (i, peer) in peers_snap.clone().into_iter().enumerate() {
                                            {
                                                let uid = peer.peer_user_id_hex.clone();
                                                let label = peer.peer_label.clone();
                                                let letter = label.chars().next().unwrap_or('?').to_string();
                                                let is_selected = selected.read().contains(&uid);
                                                let uid_for_toggle = uid.clone();
                                                rsx! {
                                                    div {
                                                        key: "{i}",
                                                        class: if is_selected {
                                                            "recovery-friend-row recovery-friend-row-selected"
                                                        } else {
                                                            "recovery-friend-row"
                                                        },
                                                        onclick: move |_| {
                                                            let mut cur = selected.read().clone();
                                                            if cur.contains(&uid_for_toggle) {
                                                                cur.remove(&uid_for_toggle);
                                                            } else {
                                                                cur.insert(uid_for_toggle.clone());
                                                            }
                                                            selected.set(cur);
                                                        },
                                                        div { class: "recovery-friend-avatar", "{letter}" }
                                                        div { class: "recovery-friend-info",
                                                            div { class: "recovery-friend-name", "{label}" }
                                                            div { class: "recovery-friend-status recovery-badge-neutral",
                                                                if is_selected { "Will ask ✓" } else { "Tap to include" }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            section { class: "recovery-section",
                                div { class: "recovery-section-num", "02 · Pieces needed" }
                                div { class: "recovery-section-hint",
                                    "This should match the number you picked when you set up "
                                    "your backup. If you don't remember, start with 3."
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
                                    span { class: "recovery-step-value", "{k_value}" }
                                    button {
                                        class: "recovery-step-btn",
                                        onclick: move |_| {
                                            let cur = *threshold.read();
                                            threshold.set(cur + 1);
                                        },
                                        "+"
                                    }
                                }
                            }
                        },

                        RecoveryPhase::Waiting | RecoveryPhase::Unlocking => rsx! {
                            section { class: "recovery-section",
                                div { class: "recovery-section-num",
                                    "Waiting for {released_count} of {k_value} friends"
                                }
                                div { class: "recovery-section-hint",
                                    "Your friends have been asked. They'll verify it's really "
                                    "you through another channel — a call, a text, in person — "
                                    "and then approve in their app."
                                }
                                {
                                    let selection = selected.read().clone();
                                    let released = released_by.read().clone();
                                    let selected_peers: Vec<OutgoingEnrollment> = peers_snap
                                        .clone()
                                        .into_iter()
                                        .filter(|p| selection.contains(&p.peer_user_id_hex))
                                        .collect();
                                    rsx! {
                                        div { class: "recovery-friend-list",
                                            for (i, peer) in selected_peers.into_iter().enumerate() {
                                                {
                                                    let has_released = released
                                                        .iter()
                                                        .any(|uid| *uid == peer.peer_user_id_hex);
                                                    let letter = peer.peer_label.chars().next().unwrap_or('?').to_string();
                                                    let badge_class = if has_released {
                                                        "recovery-friend-status recovery-badge-accepted"
                                                    } else {
                                                        "recovery-friend-status recovery-badge-pending"
                                                    };
                                                    let badge_text = if has_released {
                                                        "Approved ✓"
                                                    } else {
                                                        "Waiting..."
                                                    };
                                                    rsx! {
                                                        div {
                                                            key: "{i}",
                                                            class: "recovery-friend-row",
                                                            div { class: "recovery-friend-avatar", "{letter}" }
                                                            div { class: "recovery-friend-info",
                                                                div { class: "recovery-friend-name", "{peer.peer_label}" }
                                                                div { class: "{badge_class}", "{badge_text}" }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            if phase_now == RecoveryPhase::Unlocking {
                                div { class: "recovery-plan-pending", "Unlocking your identity..." }
                            }
                        },

                        RecoveryPhase::Done => rsx! {
                            div { class: "recovery-plan-ready",
                                "✓ You're back in. Your identity is unlocked and your "
                                "vault is syncing with your friends."
                            }
                        },
                    }

                    if let Some((ref s, is_err)) = *status.read() {
                        div {
                            class: if is_err { "recovery-status recovery-status-error" } else { "recovery-status" },
                            "{s}"
                        }
                    }
                }

                footer { class: "recovery-footer",
                    match phase_now {
                        RecoveryPhase::Picking => rsx! {
                            button {
                                class: "se-btn-glow",
                                disabled: *busy.read()
                                    || selected.read().len() < k_value as usize,
                                onclick: move |_| {
                                    let Some(net) = network.read().clone() else { return };
                                    let selection: Vec<String> = selected.read().iter().cloned().collect();
                                    busy.set(true);
                                    status.set(Some((
                                        format!("Asking {} friends...", selection.len()),
                                        false,
                                    )));
                                    spawn(async move {
                                        match recovery_bridge::initiate_recovery(net, selection).await {
                                            Ok(n) => {
                                                status.set(Some((
                                                    format!("Asked {n} friends. Waiting for them to approve."),
                                                    false,
                                                )));
                                                phase.set(RecoveryPhase::Waiting);
                                                released_by.set(Vec::new());
                                            }
                                            Err(e) => {
                                                status.set(Some((e, true)));
                                            }
                                        }
                                        busy.set(false);
                                    });
                                },
                                if *busy.read() { "Asking..." } else { "Ask for help" }
                            }
                        },
                        RecoveryPhase::Waiting => rsx! {
                            button {
                                class: "se-btn-outline",
                                onclick: move |_| {
                                    let Some(net) = network.read().clone() else { return };
                                    spawn(async move {
                                        let _ = recovery_bridge::withdraw_recovery_request(net).await;
                                        phase.set(RecoveryPhase::Picking);
                                        released_by.set(Vec::new());
                                    });
                                },
                                "Cancel"
                            }
                        },
                        RecoveryPhase::Unlocking => rsx! {},
                        RecoveryPhase::Done => rsx! {
                            button {
                                class: "se-btn-glow",
                                onclick: move |_| {
                                    state.write().show_recovery_use = false;
                                },
                                "Done"
                            }
                        },
                    }
                }
            }
        }
    }
}
