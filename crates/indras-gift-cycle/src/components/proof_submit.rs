//! Submit proof of service UI.

use dioxus::prelude::*;
use indras_sync_engine::IntentionId;

use crate::bridge::GiftCycleBridge;

/// Submit a service claim on an intention.
#[component]
pub fn ProofSubmit(
    intention_id: IntentionId,
    bridge: GiftCycleBridge,
    on_submitted: EventHandler<()>,
    on_cancel: EventHandler<()>,
) -> Element {
    let mut submitting = use_signal(|| false);
    let mut error_msg = use_signal(|| None::<String>);

    rsx! {
        div { class: "proof-submit",
            div { class: "form-header",
                h2 { class: "form-title", "Submit Proof of Service" }
                button {
                    class: "gc-btn gc-btn-outline",
                    onclick: move |_| on_cancel.call(()),
                    "Cancel"
                }
            }

            div { class: "proof-submit-info",
                div { class: "desc-panel",
                    h4 { "What happens" }
                    p {
                        "You are claiming that you have performed an act of service for this intention. "
                        "The intention creator will be able to review your claim and, if satisfied, "
                        "bless the work \u{2014} minting a Token of Gratitude for you."
                    }
                }
            }

            if let Some(err) = error_msg() {
                div { class: "form-error", "{err}" }
            }

            div { class: "form-actions",
                button {
                    class: "gc-btn gc-btn-success",
                    disabled: submitting(),
                    onclick: move |_| {
                        let b = bridge.clone();
                        submitting.set(true);
                        error_msg.set(None);
                        spawn(async move {
                            match b.submit_proof(intention_id).await {
                                Ok(_) => on_submitted.call(()),
                                Err(e) => {
                                    error_msg.set(Some(format!("{e}")));
                                    submitting.set(false);
                                }
                            }
                        });
                    },
                    if submitting() { "Submitting..." } else { "\u{1f932} Submit Proof" }
                }
            }
        }
    }
}
