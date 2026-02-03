//! Display name input screen.

use std::sync::Arc;

use dioxus::prelude::*;
use indras_network::IndrasNetwork;

use crate::state::{AsyncStatus, GenesisState};

use super::app::create_identity_and_load;

#[component]
pub fn DisplayNameScreen(
    mut state: Signal<GenesisState>,
    mut network: Signal<Option<Arc<IndrasNetwork>>>,
) -> Element {
    let status = state.read().status.clone();
    let is_loading = matches!(status, AsyncStatus::Loading);
    let name_empty = state.read().display_name.trim().is_empty();
    let can_continue = !is_loading && !name_empty;
    let error_msg = match &status {
        AsyncStatus::Error(e) => Some(e.clone()),
        _ => None,
    };

    rsx! {
        div {
            class: "genesis-screen",

            div {
                class: "genesis-card",

                h1 {
                    class: "genesis-title",
                    "What should we call you?"
                }
                p {
                    class: "genesis-hint",
                    "This name is visible to peers you connect with"
                }

                input {
                    class: "genesis-input",
                    r#type: "text",
                    placeholder: "Enter a display name...",
                    autofocus: true,
                    disabled: is_loading,
                    value: "{state.read().display_name}",
                    oninput: move |evt| {
                        state.write().display_name = evt.value();
                    },
                    onkeypress: move |evt| {
                        if evt.key() == Key::Enter && can_continue {
                            let name = state.read().display_name.clone();
                            spawn(async move {
                                create_identity_and_load(Some(name), &mut state, &mut network).await;
                            });
                        }
                    },
                }

                if let Some(err) = error_msg {
                    p {
                        class: "genesis-error",
                        "{err}"
                    }
                }

                div {
                    class: "genesis-actions",

                    button {
                        class: "genesis-btn-primary",
                        disabled: !can_continue,
                        onclick: move |_| {
                            let name = state.read().display_name.clone();
                            spawn(async move {
                                create_identity_and_load(Some(name), &mut state, &mut network).await;
                            });
                        },
                        if is_loading {
                            "Creating identity..."
                        } else {
                            "Continue"
                        }
                    }
                }
            }
        }
    }
}
