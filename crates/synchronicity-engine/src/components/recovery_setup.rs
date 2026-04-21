//! Backup-plan overlay — pick a few friends to help you recover.
//!
//! Plan-A UX. No hex, no story slot fields, no crypto vocabulary.
//! The user sees a peer list sourced from their direct-message
//! realms, taps to invite each friend as a backup, and watches
//! acceptance come in live. When enough friends accept, the actual
//! share distribution fires in the background (slice A.5, still
//! landing — for now this overlay focuses on the enrollment
//! handshake).

use std::sync::Arc;

use dioxus::prelude::*;

use indras_network::IndrasNetwork;

use crate::recovery_bridge::{self, OutgoingEnrollment};
use crate::state::AppState;
use indras_sync_engine::steward_enrollment::EnrollmentStatus;

/// Backup-plan overlay. Opened via `state.show_recovery_setup = true`.
#[component]
pub fn RecoverySetupOverlay(
    mut state: Signal<AppState>,
    network: Signal<Option<Arc<IndrasNetwork>>>,
) -> Element {
    if !state.read().show_recovery_setup {
        return rsx! {};
    }

    let mut threshold = use_signal(|| 3u8);
    let mut enrollments = use_signal(Vec::<OutgoingEnrollment>::new);
    let mut status = use_signal(|| None::<(String, bool)>);
    let mut busy_peer = use_signal(|| None::<String>);

    // Refresh the enrollment list on open. CRDT sync will bring peer
    // responses in as they arrive, but this effect only refires on
    // re-open — good enough for Plan A; a background ticker lands
    // with A.5.
    use_effect(move || {
        if let Some(net) = network.read().clone() {
            spawn(async move {
                let list = recovery_bridge::list_outgoing_enrollments(net).await;
                enrollments.set(list);
            });
        }
    });

    let peers = enrollments.read().clone();
    let accepted_count = peers.iter().filter(|e| e.status.is_accepted()).count();
    let invited_count = peers
        .iter()
        .filter(|e| matches!(e.status, EnrollmentStatus::Invited { .. }))
        .count();
    let k_value = *threshold.read();
    let max_threshold = peers.len().max(2) as u8;

    let close = move |_| {
        state.write().show_recovery_setup = false;
    };

    let ready_for_quorum = accepted_count >= k_value as usize;

    rsx! {
        div { class: "recovery-overlay",
            div { class: "recovery-modal",
                header { class: "recovery-header",
                    div { class: "recovery-eyebrow", "IF YOU LOSE ACCESS" }
                    div { class: "recovery-title", "Set up a backup plan" }
                    button { class: "recovery-close", onclick: close, "✕" }
                }

                div { class: "recovery-body",
                    div { class: "recovery-intro",
                        "Pick a few friends who can help you get back in if you ever lose "
                        "your device. Each one needs to agree. You can change your mind later."
                    }

                    // ── 01 · Pick your backup friends ─────────────────
                    section { class: "recovery-section",
                        div { class: "recovery-section-num", "01 · Your backup friends" }
                        div { class: "recovery-section-hint",
                            "These are your direct-message contacts. Tap a name to "
                            "ask them — they'll get a notification and can accept or decline."
                        }

                        if peers.is_empty() {
                            div { class: "recovery-empty",
                                "You don't have any direct-message friends yet. "
                                "Add a contact first, then come back here."
                            }
                        } else {
                            div { class: "recovery-friend-list",
                                for (i, peer) in peers.into_iter().enumerate() {
                                    {
                                        let peer_uid = peer.peer_user_id_hex.clone();
                                        let peer_label = peer.peer_label.clone();
                                        let badge_class = status_badge_class(&peer.status);
                                        let badge_text = status_badge_text(&peer.status);
                                        let letter = peer_label.chars().next().unwrap_or('?').to_string();
                                        let can_invite = matches!(
                                            peer.status,
                                            EnrollmentStatus::NotInvited
                                                | EnrollmentStatus::Declined { .. }
                                                | EnrollmentStatus::Withdrawn
                                        );
                                        let can_revoke = matches!(
                                            peer.status,
                                            EnrollmentStatus::Invited { .. }
                                                | EnrollmentStatus::Accepted { .. }
                                        );
                                        let is_busy = busy_peer.read().as_deref() == Some(peer_uid.as_str());

                                        let uid_for_invite = peer_uid.clone();
                                        let label_for_invite = peer_label.clone();
                                        let uid_for_revoke = peer_uid.clone();
                                        let label_for_revoke = peer_label.clone();

                                        rsx! {
                                            div {
                                                key: "{i}",
                                                class: "recovery-friend-row",
                                                div { class: "recovery-friend-avatar", "{letter}" }
                                                div { class: "recovery-friend-info",
                                                    div { class: "recovery-friend-name", "{peer_label}" }
                                                    div {
                                                        class: "recovery-friend-status {badge_class}",
                                                        "{badge_text}"
                                                    }
                                                }
                                                div { class: "recovery-friend-actions",
                                                    if can_invite {
                                                        button {
                                                            class: "se-btn-outline",
                                                            disabled: is_busy,
                                                            onclick: move |_| {
                                                                let Some(net) = network.read().clone() else { return };
                                                                let k = *threshold.read();
                                                                let n = enrollments.read().len() as u8;
                                                                let uid = uid_for_invite.clone();
                                                                let label = label_for_invite.clone();
                                                                busy_peer.set(Some(uid.clone()));
                                                                status.set(Some((format!("Asking {label}..."), false)));
                                                                spawn(async move {
                                                                    let result = recovery_bridge::invite_steward(
                                                                        net.clone(), &uid, k, n, None
                                                                    ).await;
                                                                    match result {
                                                                        Ok(()) => {
                                                                            status.set(Some((
                                                                                format!("Asked {label}. Waiting for their reply."),
                                                                                false,
                                                                            )));
                                                                            let fresh = recovery_bridge::list_outgoing_enrollments(net).await;
                                                                            enrollments.set(fresh);
                                                                        }
                                                                        Err(e) => {
                                                                            status.set(Some((e, true)));
                                                                        }
                                                                    }
                                                                    busy_peer.set(None);
                                                                });
                                                            },
                                                            if is_busy { "..." } else { "Ask" }
                                                        }
                                                    }
                                                    if can_revoke {
                                                        button {
                                                            class: "se-btn-text recovery-friend-revoke",
                                                            disabled: is_busy,
                                                            onclick: move |_| {
                                                                let Some(net) = network.read().clone() else { return };
                                                                let uid = uid_for_revoke.clone();
                                                                let label = label_for_revoke.clone();
                                                                busy_peer.set(Some(uid.clone()));
                                                                spawn(async move {
                                                                    let result = recovery_bridge::revoke_invitation(net.clone(), &uid).await;
                                                                    match result {
                                                                        Ok(()) => {
                                                                            status.set(Some((
                                                                                format!("{label} removed from your backup plan."),
                                                                                false,
                                                                            )));
                                                                            let fresh = recovery_bridge::list_outgoing_enrollments(net).await;
                                                                            enrollments.set(fresh);
                                                                        }
                                                                        Err(e) => {
                                                                            status.set(Some((e, true)));
                                                                        }
                                                                    }
                                                                    busy_peer.set(None);
                                                                });
                                                            },
                                                            "Remove"
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

                    // ── 02 · How many must help ───────────────────────
                    section { class: "recovery-section",
                        div { class: "recovery-section-num", "02 · How many must agree" }
                        div { class: "recovery-section-hint",
                            "If you ever lose your device, this many friends need to "
                            "agree it's really you before you get back in. Higher is safer."
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
                            span { class: "recovery-step-value", "{k_value} of {max_threshold}" }
                            button {
                                class: "recovery-step-btn",
                                disabled: (k_value as usize) >= (max_threshold as usize),
                                onclick: move |_| {
                                    let cur = *threshold.read();
                                    if (cur as usize) < (max_threshold as usize) { threshold.set(cur + 1); }
                                },
                                "+"
                            }
                        }
                    }

                    // ── 03 · What they'll do ──────────────────────────
                    section { class: "recovery-section",
                        div { class: "recovery-section-num", "03 · What they'll do" }
                        div { class: "recovery-section-hint",
                            "If you lose your device, you'll reach out to them through "
                            "another channel — a call, a text, in person. Once they're "
                            "sure it's really you, they tap Approve in their app and you're back in."
                        }
                    }

                    // ── Status ────────────────────────────────────────
                    if let Some((ref s, is_err)) = *status.read() {
                        div {
                            class: if is_err { "recovery-status recovery-status-error" } else { "recovery-status" },
                            "{s}"
                        }
                    }

                    // ── Plan state summary ────────────────────────────
                    div { class: "recovery-plan-summary",
                        if ready_for_quorum {
                            div { class: "recovery-plan-ready",
                                "✓ You're covered. {accepted_count} friends have agreed "
                                "— any {k_value} of them can help you recover."
                            }
                        } else if invited_count > 0 {
                            div { class: "recovery-plan-pending",
                                "Waiting on {k_value - accepted_count as u8} more friends to agree. "
                                "{accepted_count} of {k_value} so far."
                            }
                        } else {
                            div { class: "recovery-plan-empty",
                                "Ask at least {k_value} friends to cover your backup."
                            }
                        }
                    }
                }

                footer { class: "recovery-footer" }
            }
        }
    }
}

fn status_badge_class(status: &EnrollmentStatus) -> &'static str {
    match status {
        EnrollmentStatus::NotInvited => "recovery-badge-neutral",
        EnrollmentStatus::Invited { .. } => "recovery-badge-pending",
        EnrollmentStatus::Accepted { .. } => "recovery-badge-accepted",
        EnrollmentStatus::Declined { .. } => "recovery-badge-declined",
        EnrollmentStatus::Withdrawn => "recovery-badge-neutral",
    }
}

fn status_badge_text(status: &EnrollmentStatus) -> String {
    match status {
        EnrollmentStatus::NotInvited => "Not asked yet".to_string(),
        EnrollmentStatus::Invited { .. } => "Asked — waiting".to_string(),
        EnrollmentStatus::Accepted { .. } => "Agreed ✓".to_string(),
        EnrollmentStatus::Declined { .. } => "Declined".to_string(),
        EnrollmentStatus::Withdrawn => "Removed".to_string(),
    }
}
