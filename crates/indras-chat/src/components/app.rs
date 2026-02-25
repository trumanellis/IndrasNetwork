//! Root app component with first-run detection.

use std::sync::Arc;
use dioxus::prelude::*;
use crate::state::{AppPhase, ChatContext, ConversationSummary};
use crate::bridge;
use indras_peering::{PeerEvent, PeeringRuntime};

/// Root application component.
#[component]
pub fn App() -> Element {
    let mut phase = use_signal(|| AppPhase::Loading);
    let mut error = use_signal(|| None::<String>);
    let mut loading = use_signal(|| false);

    // First-run detection on mount
    let phase_clone = phase;
    use_effect(move || {
        if *phase_clone.read() == AppPhase::Loading {
            if bridge::is_first_run() {
                phase.set(AppPhase::Setup);
            } else {
                spawn(async move {
                    match bridge::load_identity().await {
                        Ok(runtime) => {
                            phase.set(AppPhase::Running(runtime));
                        }
                        Err(e) => {
                            error.set(Some(e));
                            phase.set(AppPhase::Setup);
                        }
                    }
                });
            }
        }
    });

    let current_phase = phase.read().clone();

    match current_phase {
        AppPhase::Loading => rsx! {
            div { class: "loading-screen",
                div { class: "loading-logo", "I" }
                div { class: "loading-text", "Loading..." }
            }
        },
        AppPhase::Setup => rsx! {
            super::setup::SetupView {
                on_create: move |(name, slots): (String, Option<[String; 23]>)| {
                    loading.set(true);
                    error.set(None);
                    spawn(async move {
                        match bridge::create_identity(&name, slots).await {
                            Ok(runtime) => {
                                phase.set(AppPhase::Running(runtime));
                            }
                            Err(e) => {
                                error.set(Some(e));
                                loading.set(false);
                            }
                        }
                    });
                },
                error: error.read().clone(),
                loading: *loading.read(),
            }
        },
        AppPhase::Running(runtime) => rsx! {
            MainLayout { runtime: PeeringArc(runtime) }
        },
    }
}

/// Newtype wrapper so `Arc<PeeringRuntime>` satisfies Dioxus `#[component]`'s `PartialEq` bound.
/// Equality is by pointer identity.
#[derive(Clone)]
pub struct PeeringArc(pub Arc<PeeringRuntime>);

impl PartialEq for PeeringArc {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

/// Main chat layout with shared context (standalone mode — owns shutdown).
#[component]
fn MainLayout(runtime: PeeringArc) -> Element {
    let runtime = runtime.0;

    // Provide shared chat context
    let mut ctx = use_context_provider(|| ChatContext {
        runtime: Signal::new(runtime.clone()),
        active_chat: Signal::new(None),
        conversations: Signal::new(Vec::new()),
        peers: Signal::new(Vec::new()),
        show_add_contact: Signal::new(false),
        typing_peers: Signal::new(Vec::new()),
    });

    // Spawn event consumer loop driven by PeeringRuntime events
    let rt = runtime.clone();
    use_effect(move || {
        let rt = rt.clone();
        spawn(async move {
            // Initial conversation refresh
            refresh_conversations(&rt, ctx.conversations).await;

            // Atomically subscribe + snapshot to avoid race
            let (mut rx, initial_peers) = rt.subscribe_with_snapshot();
            if !initial_peers.is_empty() {
                ctx.peers.set(initial_peers);
            }
            loop {
                match rx.recv().await {
                    Ok(event) => match event {
                        PeerEvent::PeersChanged { peers } => {
                            ctx.peers.set(peers);
                        }
                        PeerEvent::ConversationOpened { .. } => {
                            refresh_conversations(&rt, ctx.conversations).await;
                        }
                        PeerEvent::PeerConnected { .. } | PeerEvent::PeerDisconnected { .. } => {
                            // Peer list changes are handled by PeersChanged
                        }
                        PeerEvent::NetworkEvent(_) => {
                            // Could trigger conversation refresh for new messages
                        }
                        _ => {}
                    },
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(skipped = n, "event consumer lagged");
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });
    });

    rsx! {
        div { class: "main-layout",
            super::sidebar::Sidebar {}
            super::chat_view::ChatView {}

            if *ctx.show_add_contact.read() {
                super::contact_add::ContactAdd {
                    on_close: move |_| {
                        ctx.show_add_contact.set(false);
                        // Refresh conversations after adding contact
                        let rt = ctx.runtime.read().clone();
                        spawn(async move {
                            refresh_conversations(&rt, ctx.conversations).await;
                        });
                    },
                }
            }
        }
    }
}

/// Embeddable chat layout for use in other apps (e.g., indras-workspace).
/// Accepts a `PeeringRuntime` instance — no identity/setup flow.
#[component]
pub fn ChatLayout(runtime: PeeringArc) -> Element {
    let runtime = runtime.0;

    // Provide shared chat context
    let mut ctx = use_context_provider(|| ChatContext {
        runtime: Signal::new(runtime.clone()),
        active_chat: Signal::new(None),
        conversations: Signal::new(Vec::new()),
        peers: Signal::new(Vec::new()),
        show_add_contact: Signal::new(false),
        typing_peers: Signal::new(Vec::new()),
    });

    // Spawn event consumer loop
    let rt = runtime.clone();
    use_effect(move || {
        let rt = rt.clone();
        spawn(async move {
            refresh_conversations(&rt, ctx.conversations).await;

            // Atomically subscribe + snapshot to avoid race
            let (mut rx, initial_peers) = rt.subscribe_with_snapshot();
            if !initial_peers.is_empty() {
                ctx.peers.set(initial_peers);
            }
            loop {
                match rx.recv().await {
                    Ok(event) => match event {
                        PeerEvent::PeersChanged { peers } => {
                            ctx.peers.set(peers);
                        }
                        PeerEvent::ConversationOpened { .. } => {
                            refresh_conversations(&rt, ctx.conversations).await;
                        }
                        _ => {}
                    },
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(skipped = n, "event consumer lagged");
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });
    });

    rsx! {
        div { class: "main-layout",
            super::sidebar::Sidebar {}
            super::chat_view::ChatView {}

            if *ctx.show_add_contact.read() {
                super::contact_add::ContactAdd {
                    on_close: move |_| {
                        ctx.show_add_contact.set(false);
                        let rt = ctx.runtime.read().clone();
                        spawn(async move {
                            refresh_conversations(&rt, ctx.conversations).await;
                        });
                    },
                }
            }
        }
    }
}

/// Refresh the conversation list from network realms.
async fn refresh_conversations(
    runtime: &PeeringRuntime,
    mut conversations: Signal<Vec<ConversationSummary>>,
) {
    let network = runtime.network();
    let realm_ids = network.realms();
    let mut convos = Vec::new();

    for realm_id in realm_ids {
        if let Some(realm) = network.get_realm_by_id(&realm_id) {
            let display_name = realm.name()
                .map(|s| s.to_string())
                .unwrap_or_else(|| {
                    let id_hex = hex::encode(realm_id.as_bytes());
                    format!("Chat {}", &id_hex[..8])
                });

            // Try to get last message preview
            let (last_msg, last_time) = match realm.chat_doc().await {
                Ok(doc) => {
                    let state = doc.read().await;
                    let sorted = state.visible_messages();
                    if let Some(msg) = sorted.last() {
                        let preview = if msg.current_content.len() > 50 {
                            format!("{}...", &msg.current_content[..50])
                        } else {
                            msg.current_content.clone()
                        };
                        (Some(preview), Some(msg.created_at))
                    } else {
                        (None, None)
                    }
                }
                Err(_) => (None, None),
            };

            convos.push(ConversationSummary {
                realm_id,
                display_name,
                last_message: last_msg,
                last_message_time: last_time,
                unread_count: 0,
            });
        }
    }

    // Sort by last message time descending
    convos.sort_by(|a, b| b.last_message_time.cmp(&a.last_message_time));
    conversations.set(convos);
}
