//! Contact invite overlay component.
//!
//! Purely presentational — no `indras-network` dependency.
//! Takes primitive signals for all data and event handlers for actions.

use dioxus::prelude::*;

/// Overlay for sharing and accepting contact invite links.
///
/// Follows the same overlay pattern as `MarkdownPreviewOverlay`:
/// backdrop click closes, stop propagation on dialog, close button.
#[component]
pub fn ContactInviteOverlay(
    mut is_open: Signal<bool>,
    /// The full `syncengine:contact:...` URI for this user.
    invite_uri: ReadSignal<String>,
    /// This user's display name.
    display_name: ReadSignal<String>,
    /// Short hex member ID snippet.
    member_id_short: ReadSignal<String>,
    /// Text input for pasting another user's invite URI.
    mut connect_input: Signal<String>,
    /// Connect status: None=idle, Some("error:...") or Some("success:...").
    connect_status: ReadSignal<Option<String>>,
    /// Parsed inviter name from pasted URI (live preview).
    parsed_inviter_name: ReadSignal<Option<String>>,
    /// Fires with the pasted URI string when user clicks "Connect".
    on_connect: EventHandler<String>,
    /// Fires on input change for live parsing of pasted URI.
    on_parse_input: EventHandler<String>,
    /// True briefly after copy to show "Copied!" feedback.
    copy_feedback: ReadSignal<bool>,
    /// Fires when user clicks the copy button.
    on_copy: EventHandler<()>,
) -> Element {
    if !is_open() {
        return rsx! {};
    }

    let uri = invite_uri();
    let name = display_name();
    let mid = member_id_short();
    let input_val = connect_input();
    let status = connect_status();
    let parsed_name = parsed_inviter_name();
    let copied = copy_feedback();

    // Determine status display
    let status_class = match &status {
        Some(s) if s.starts_with("error:") => Some("contact-invite-status-error"),
        Some(s) if s.starts_with("success:") => Some("contact-invite-status-success"),
        _ => None,
    };
    let status_text = match &status {
        Some(s) if s.starts_with("error:") => Some(s.strip_prefix("error:").unwrap_or(s).to_string()),
        Some(s) if s.starts_with("success:") => Some(s.strip_prefix("success:").unwrap_or(s).to_string()),
        _ => None,
    };

    rsx! {
        div {
            class: "contact-invite-overlay",
            onclick: move |_| is_open.set(false),

            div {
                class: "contact-invite-dialog",
                onclick: move |e| e.stop_propagation(),

                // Header
                div {
                    class: "contact-invite-header",
                    h2 { "Connections" }
                    button {
                        class: "contact-invite-close",
                        onclick: move |_| is_open.set(false),
                        "\u{00d7}"
                    }
                }

                // Content
                div {
                    class: "contact-invite-content",

                    // Share section
                    section {
                        class: "contact-invite-share",
                        h3 { "Share Your Link" }
                        p {
                            class: "contact-invite-identity",
                            span { class: "contact-invite-name", "{name}" }
                            " "
                            span { class: "contact-invite-mid", "{mid}" }
                        }
                        div {
                            class: "contact-invite-uri",
                            "{uri}"
                        }
                        button {
                            class: "contact-invite-copy-btn",
                            onclick: move |_| on_copy.call(()),
                            if copied { "Copied!" } else { "Copy Link" }
                        }
                    }

                    // Connect section
                    section {
                        class: "contact-invite-connect",
                        h3 { "Connect with Someone" }
                        input {
                            class: "contact-invite-input",
                            r#type: "text",
                            placeholder: "Paste a syncengine:contact:... link",
                            value: "{input_val}",
                            oninput: move |evt| {
                                let val = evt.value();
                                connect_input.set(val.clone());
                                on_parse_input.call(val);
                            },
                        }

                        if let Some(ref inviter) = parsed_name {
                            div {
                                class: "contact-invite-preview",
                                "Invite from: {inviter}"
                            }
                        }

                        if let (Some(cls), Some(txt)) = (status_class, &status_text) {
                            div {
                                class: "{cls}",
                                "{txt}"
                            }
                        }

                        button {
                            class: "contact-invite-connect-btn",
                            disabled: input_val.trim().is_empty(),
                            onclick: move |_| {
                                on_connect.call(connect_input());
                            },
                            "Connect"
                        }
                    }
                }
            }
        }
    }
}
