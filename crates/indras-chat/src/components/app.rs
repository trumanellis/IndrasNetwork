//! Root app component with first-run detection.

use std::sync::Arc;
use dioxus::prelude::*;
use crate::state::{AppPhase, ChatContext, ConversationSummary};
use crate::bridge::{self, NetworkHandle};
use indras_network::IndrasNetwork;

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
                        Ok(handle) => {
                            phase.set(AppPhase::Running(Arc::new(handle)));
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
                            Ok(handle) => {
                                phase.set(AppPhase::Running(Arc::new(handle)));
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
        AppPhase::Running(handle) => rsx! {
            MainLayout { handle }
        },
    }
}

/// Main chat layout with shared context.
#[component]
fn MainLayout(handle: Arc<NetworkHandle>) -> Element {
    // Provide shared chat context
    let mut ctx = use_context_provider(|| ChatContext {
        handle: Signal::new(handle.clone()),
        active_chat: Signal::new(None),
        conversations: Signal::new(Vec::new()),
        show_add_contact: Signal::new(false),
        typing_peers: Signal::new(Vec::new()),
    });

    // Spawn background task to populate conversations
    let net = handle.clone();
    use_effect(move || {
        let net = net.clone();
        spawn(async move {
            refresh_conversations(&net, ctx.conversations).await;
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
                        let net = ctx.handle.read().clone();
                        spawn(async move {
                            refresh_conversations(&net, ctx.conversations).await;
                        });
                    },
                }
            }
        }
    }
}

/// Newtype wrapper so `Arc<IndrasNetwork>` satisfies Dioxus `#[component]`'s `PartialEq` bound.
/// Equality is by pointer identity.
#[derive(Clone)]
pub struct NetworkArc(pub Arc<IndrasNetwork>);

impl PartialEq for NetworkArc {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

/// Embeddable chat layout for use in other apps (e.g., indras-workspace).
/// Accepts a raw IndrasNetwork instance — no identity/setup flow.
#[component]
pub fn ChatLayout(network: NetworkArc) -> Element {
    let network = network.0;
    let handle = Arc::new(NetworkHandle { network });

    // Provide shared chat context
    let mut ctx = use_context_provider(|| ChatContext {
        handle: Signal::new(handle.clone()),
        active_chat: Signal::new(None),
        conversations: Signal::new(Vec::new()),
        show_add_contact: Signal::new(false),
        typing_peers: Signal::new(Vec::new()),
    });

    // Spawn background task to populate conversations
    let net = handle.clone();
    use_effect(move || {
        let net = net.clone();
        spawn(async move {
            refresh_conversations(&net, ctx.conversations).await;
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
                        let net = ctx.handle.read().clone();
                        spawn(async move {
                            refresh_conversations(&net, ctx.conversations).await;
                        });
                    },
                }
            }
        }
    }
}

/// Refresh the conversation list from network realms.
async fn refresh_conversations(
    handle: &NetworkHandle,
    mut conversations: Signal<Vec<ConversationSummary>>,
) {
    let realm_ids = handle.network.realms();
    let mut convos = Vec::new();

    for realm_id in realm_ids {
        if let Some(realm) = handle.network.get_realm_by_id(&realm_id) {
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
