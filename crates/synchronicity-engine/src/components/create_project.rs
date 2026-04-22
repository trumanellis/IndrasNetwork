//! Create project overlay — name input for creating a new Project under a
//! parent realm.
//!
//! Modelled 1:1 on [`super::create_realm::CreateRealmOverlay`]. The overlay
//! is a scaffold — it is **not** wired into any button or menu yet. The final
//! layout decision (5th column vs nested vs shared Group column) determines
//! where the affordance lives; `CreateProjectOverlay` will be wired in then.

use std::sync::Arc;

use dioxus::prelude::*;

use crate::state::RealmId;
use crate::vault_manager::VaultManager;

/// Overlay for creating a new Project under `parent_realm`.
///
/// # Behaviour
///
/// - Autofocus name input; Enter submits, Esc cancels.
/// - On success: calls `on_close(Some(new_project_id))`.
/// - On cancel or Esc: calls `on_close(None)`.
/// - No explicit "Create" button is required — Enter suffices — but a button
///   is included for mouse users, mirroring `CreateRealmOverlay`.
#[component]
pub fn CreateProjectOverlay(
    vault_manager: Signal<Option<Arc<VaultManager>>>,
    /// The realm under whose vault directory the project folder will be
    /// created (passed straight through to
    /// [`VaultManager::create_project`]).
    parent_realm: RealmId,
    /// Called with `Some(project_id)` on success, `None` on cancel/Esc.
    on_close: EventHandler<Option<[u8; 32]>>,
) -> Element {
    let mut name_input = use_signal(String::new);
    let mut status = use_signal(|| None::<String>);

    let name_val = name_input();
    let in_flight = status.read().is_some();
    let can_create = !in_flight && !name_val.trim().is_empty();

    let on_create = use_callback(move |_: ()| {
        if status.read().is_some() {
            return;
        }
        let name = name_input.read().trim().to_string();
        if name.is_empty() {
            return;
        }
        let vm_opt = vault_manager.read().clone();
        status.set(Some("Creating...".to_string()));
        spawn(async move {
            let Some(vm) = vm_opt else {
                status.set(Some("error:vault manager not ready".into()));
                tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
                status.set(None);
                on_close.call(None);
                return;
            };
            match vm.create_project(&parent_realm, &name).await {
                Ok(info) => {
                    status.set(Some(format!("success:Created \"{}\"!", name)));
                    tokio::time::sleep(std::time::Duration::from_millis(800)).await;
                    status.set(None);
                    on_close.call(Some(info.id));
                }
                Err(e) => {
                    status.set(Some(format!("error:Failed: {e}")));
                    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
                    status.set(None);
                    on_close.call(None);
                }
            }
        });
    });

    let status_val = status();
    let status_class = match &status_val {
        Some(s) if s.starts_with("error:") => Some("contact-invite-status-error"),
        Some(s) if s.starts_with("success:") => Some("contact-invite-status-success"),
        _ => None,
    };
    let status_text = match &status_val {
        Some(s) if s.starts_with("error:") => Some(s.strip_prefix("error:").unwrap_or(s).to_string()),
        Some(s) if s.starts_with("success:") => Some(s.strip_prefix("success:").unwrap_or(s).to_string()),
        Some(s) => Some(s.clone()),
        _ => None,
    };

    rsx! {
        div {
            class: "contact-invite-overlay",
            onclick: move |_| on_close.call(None),

            div {
                class: "contact-invite-dialog",
                role: "dialog",
                "aria-modal": "true",
                onclick: move |e| e.stop_propagation(),

                // Header
                div {
                    class: "contact-invite-header",
                    h2 { "New Project" }
                    button {
                        class: "contact-invite-close",
                        "aria-label": "Close",
                        onclick: move |_| on_close.call(None),
                        "\u{00d7}"
                    }
                }

                // Content
                div {
                    class: "contact-invite-content",

                    // Name input
                    section {
                        class: "contact-invite-share",
                        h3 { "Project Name" }
                        input {
                            class: "contact-invite-input",
                            r#type: "text",
                            placeholder: "e.g. Sprint 1",
                            "aria-label": "Project name",
                            value: "{name_val}",
                            autofocus: true,
                            oninput: move |evt| name_input.set(evt.value()),
                            onkeydown: move |e: KeyboardEvent| {
                                if e.key() == Key::Enter {
                                    on_create(());
                                } else if e.key() == Key::Escape {
                                    on_close.call(None);
                                }
                            },
                        }
                    }

                    // Status
                    if let (Some(cls), Some(txt)) = (status_class, &status_text) {
                        div {
                            class: "{cls}",
                            role: "alert",
                            "{txt}"
                        }
                    }

                    // Create button
                    button {
                        class: "contact-invite-connect-btn",
                        disabled: !can_create,
                        onclick: move |_| on_create(()),
                        if in_flight { "Creating\u{2026}" } else { "Create" }
                    }
                }
            }
        }
    }
}
