//! Steward inbox overlay — incoming invitations + (later) recovery
//! requests.
//!
//! When another user invites this device to be a backup friend, the
//! invitation lands as a CRDT doc in the DM realm they share. This
//! overlay surfaces every such pending invitation as a plain-language
//! approve/decline dialog. The underlying response doc is written
//! back via [`crate::recovery_bridge::respond_to_invitation`].
//!
//! Recovery-request approvals land in slice A.6 and will appear in a
//! second section of this same overlay.

use std::sync::Arc;

use dioxus::prelude::*;

use indras_network::IndrasNetwork;

use crate::recovery_bridge::{self, IncomingInvitation};
use crate::state::AppState;

/// Steward inbox. Opened via `state.show_steward_inbox = true`.
#[component]
pub fn StewardInboxOverlay(
    mut state: Signal<AppState>,
    network: Signal<Option<Arc<IndrasNetwork>>>,
) -> Element {
    if !state.read().show_steward_inbox {
        return rsx! {};
    }

    let mut invitations = use_signal(Vec::<IncomingInvitation>::new);
    let mut status = use_signal(|| None::<(String, bool)>);
    let mut busy_sender = use_signal(|| None::<String>);

    use_effect(move || {
        if let Some(net) = network.read().clone() {
            spawn(async move {
                let list = recovery_bridge::list_incoming_invitations(net).await;
                let pending = list.iter().filter(|i| !i.already_responded).count();
                state.write().steward_inbox_pending = pending;
                invitations.set(list);
            });
        }
    });

    let close = move |_| {
        state.write().show_steward_inbox = false;
    };

    let snapshot = invitations.read().clone();
    let (pending, standing): (Vec<IncomingInvitation>, Vec<IncomingInvitation>) =
        snapshot.into_iter().partition(|i| !i.already_responded);
    let standing_accepts: Vec<IncomingInvitation> =
        standing.into_iter().filter(|i| i.last_response_accepted).collect();

    rsx! {
        div { class: "recovery-overlay",
            div { class: "recovery-modal",
                header { class: "recovery-header",
                    div { class: "recovery-eyebrow", "YOUR INBOX" }
                    div { class: "recovery-title", "Requests from friends" }
                    button { class: "recovery-close", onclick: close, "✕" }
                }

                div { class: "recovery-body",
                    // ── Pending invitations ───────────────────────────
                    section { class: "recovery-section",
                        div { class: "recovery-section-num",
                            "01 · People asking you to be a backup friend"
                        }
                        if pending.is_empty() {
                            div { class: "recovery-empty",
                                "No new requests. You're all caught up."
                            }
                        } else {
                            div { class: "recovery-friend-list",
                                for (i, inv) in pending.into_iter().enumerate() {
                                    {
                                        let from_uid = inv.from_user_id_hex.clone();
                                        let from_name = if inv.from_display_name.trim().is_empty() {
                                            format!("Peer {}", &inv.from_user_id_hex[..8])
                                        } else {
                                            inv.from_display_name.clone()
                                        };
                                        let letter = from_name.chars().next().unwrap_or('?').to_string();
                                        let responsibility = inv.responsibility_text.clone();
                                        let threshold_k = inv.threshold_k;
                                        let total_n = inv.total_n;
                                        let is_busy = busy_sender.read().as_deref() == Some(from_uid.as_str());

                                        let uid_for_accept = from_uid.clone();
                                        let uid_for_decline = from_uid.clone();
                                        let name_for_accept = from_name.clone();
                                        let name_for_decline = from_name.clone();

                                        rsx! {
                                            div {
                                                key: "{i}",
                                                class: "steward-invite-card",
                                                div { class: "steward-invite-head",
                                                    div { class: "recovery-friend-avatar", "{letter}" }
                                                    div { class: "steward-invite-heading",
                                                        div { class: "recovery-friend-name", "{from_name}" }
                                                        div { class: "steward-invite-meta",
                                                            "asks for {threshold_k} of {total_n} friends"
                                                        }
                                                    }
                                                }
                                                div { class: "steward-invite-body",
                                                    "{responsibility}"
                                                }
                                                div { class: "steward-invite-actions",
                                                    button {
                                                        class: "se-btn-glow",
                                                        disabled: is_busy,
                                                        onclick: move |_| {
                                                            let Some(net) = network.read().clone() else { return };
                                                            let uid = uid_for_accept.clone();
                                                            let label = name_for_accept.clone();
                                                            busy_sender.set(Some(uid.clone()));
                                                            spawn(async move {
                                                                let res = recovery_bridge::respond_to_invitation(
                                                                    net.clone(), &uid, true
                                                                ).await;
                                                                match res {
                                                                    Ok(()) => {
                                                                        status.set(Some((
                                                                            format!("You're now a backup friend for {label}."),
                                                                            false,
                                                                        )));
                                                                        let fresh = recovery_bridge::list_incoming_invitations(net).await;
                                                                        let pending_n = fresh.iter().filter(|i| !i.already_responded).count();
                                                                        state.write().steward_inbox_pending = pending_n;
                                                                        invitations.set(fresh);
                                                                    }
                                                                    Err(e) => status.set(Some((e, true))),
                                                                }
                                                                busy_sender.set(None);
                                                            });
                                                        },
                                                        if is_busy { "..." } else { "Accept" }
                                                    }
                                                    button {
                                                        class: "se-btn-outline",
                                                        disabled: is_busy,
                                                        onclick: move |_| {
                                                            let Some(net) = network.read().clone() else { return };
                                                            let uid = uid_for_decline.clone();
                                                            let label = name_for_decline.clone();
                                                            busy_sender.set(Some(uid.clone()));
                                                            spawn(async move {
                                                                let res = recovery_bridge::respond_to_invitation(
                                                                    net.clone(), &uid, false
                                                                ).await;
                                                                match res {
                                                                    Ok(()) => {
                                                                        status.set(Some((
                                                                            format!("Declined {label}'s request."),
                                                                            false,
                                                                        )));
                                                                        let fresh = recovery_bridge::list_incoming_invitations(net).await;
                                                                        let pending_n = fresh.iter().filter(|i| !i.already_responded).count();
                                                                        state.write().steward_inbox_pending = pending_n;
                                                                        invitations.set(fresh);
                                                                    }
                                                                    Err(e) => status.set(Some((e, true))),
                                                                }
                                                                busy_sender.set(None);
                                                            });
                                                        },
                                                        if is_busy { "..." } else { "Decline" }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // ── Standing-accepted ─────────────────────────────
                    section { class: "recovery-section",
                        div { class: "recovery-section-num",
                            "02 · You're a backup friend for"
                        }
                        if standing_accepts.is_empty() {
                            div { class: "recovery-empty",
                                "You haven't accepted any invitations yet."
                            }
                        } else {
                            div { class: "recovery-friend-list",
                                for (i, inv) in standing_accepts.into_iter().enumerate() {
                                    {
                                        let from_name = if inv.from_display_name.trim().is_empty() {
                                            format!("Peer {}", &inv.from_user_id_hex[..8])
                                        } else {
                                            inv.from_display_name.clone()
                                        };
                                        let letter = from_name.chars().next().unwrap_or('?').to_string();

                                        rsx! {
                                            div {
                                                key: "{i}",
                                                class: "recovery-friend-row",
                                                div { class: "recovery-friend-avatar", "{letter}" }
                                                div { class: "recovery-friend-info",
                                                    div { class: "recovery-friend-name", "{from_name}" }
                                                    div { class: "recovery-friend-status recovery-badge-accepted",
                                                        "You'll verify them if they ask"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // ── Recovery requests (A.6 placeholder) ───────────
                    // section { class: "recovery-section",
                    //     div { class: "recovery-section-num",
                    //         "03 · Friends asking for help recovering"
                    //     }
                    //     Renders once slice A.6 (recovery request protocol) lands.
                    // }

                    if let Some((ref s, is_err)) = *status.read() {
                        div {
                            class: if is_err { "recovery-status recovery-status-error" } else { "recovery-status" },
                            "{s}"
                        }
                    }
                }

                footer { class: "recovery-footer" }
            }
        }
    }
}
