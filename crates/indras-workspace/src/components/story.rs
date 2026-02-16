//! Story (conversation/chat) view component.

use dioxus::prelude::*;

/// Embedded artifact reference in a story message.
#[derive(Clone, Debug, PartialEq)]
pub struct StoryArtifactRef {
    pub icon: String,
    pub name: String,
    pub artifact_type: String,
}

/// A message in a story thread.
#[derive(Clone, Debug, PartialEq)]
pub struct StoryMessage {
    pub sender_name: String,
    pub sender_letter: String,
    pub sender_color_class: String,
    pub content: String,
    pub time: String,
    pub is_self: bool,
    /// Embedded artifact card
    pub artifact_ref: Option<StoryArtifactRef>,
    /// Image attachment placeholder
    pub image_ref: Option<bool>,
    /// Branch indicator text
    pub branch_label: Option<String>,
    /// Day separator to render before this message
    pub day_separator: Option<String>,
}

/// Story view — chat-style conversation thread.
#[component]
pub fn StoryView(
    title: String,
    audience_count: usize,
    message_count: usize,
    messages: Vec<StoryMessage>,
    on_artifact_click: Option<EventHandler<StoryArtifactRef>>,
    #[props(optional)]
    on_send: Option<EventHandler<String>>,
) -> Element {
    let audience_text = format!("{} audience", audience_count);
    let msg_text = format!("{} messages", message_count);
    let mut compose_text = use_signal(String::new);

    rsx! {
        div {
            class: "view active",
            div {
                class: "story-scroll",
                div {
                    class: "story-header",
                    div { class: "story-title", "{title}" }
                    div {
                        class: "story-meta",
                        span { class: "doc-meta-tag type-story", "Story" }
                        span { "{audience_text}" }
                        span { "\u{00B7}" }
                        span { "{msg_text}" }
                    }
                }
                div {
                    class: "story-messages",
                    for msg in messages.iter() {
                        // Day separator before message if present
                        if let Some(day) = &msg.day_separator {
                            div {
                                class: "story-day-sep",
                                span { "{day}" }
                            }
                        }
                        {render_message(msg, &on_artifact_click)}
                    }
                }
            }
            // Compose bar
            div {
                class: "compose-bar",
                button { class: "compose-attach", "\u{1F4CE}" }
                textarea {
                    class: "compose-input",
                    placeholder: "Send a message...",
                    rows: "1",
                    value: "{compose_text}",
                    oninput: move |evt| {
                        compose_text.set(evt.value().clone());
                    },
                }
                button {
                    class: "compose-btn",
                    onclick: move |_| {
                        let text = compose_text.read().trim().to_string();
                        if !text.is_empty() {
                            if let Some(handler) = &on_send {
                                handler.call(text);
                            }
                            compose_text.set(String::new());
                        }
                    },
                    "\u{27A4}"
                }
            }
        }
    }
}

fn render_message(msg: &StoryMessage, on_artifact_click: &Option<EventHandler<StoryArtifactRef>>) -> Element {
    let msg_class = if msg.is_self { "msg self" } else { "msg" };
    let avatar_class = if msg.is_self {
        "msg-avatar".to_string()
    } else {
        format!("msg-avatar {}", msg.sender_color_class)
    };
    let avatar_style = if msg.is_self {
        "background:linear-gradient(135deg,var(--accent-teal),var(--accent-violet));color:var(--bg-void)"
    } else {
        ""
    };

    rsx! {
        div {
            class: "{msg_class}",
            div {
                class: "{avatar_class}",
                style: "{avatar_style}",
                "{msg.sender_letter}"
            }
            div {
                class: "msg-body",
                div { class: "msg-sender", "{msg.sender_name}" }
                div {
                    class: "msg-bubble",
                    "{msg.content}"
                    // Artifact card embed
                    if let Some(aref) = &msg.artifact_ref {
                        {
                            let click_ref = aref.clone();
                            let handler = on_artifact_click.clone();
                            rsx! {
                                div {
                                    class: "msg-artifact-card",
                                    onclick: move |_| {
                                        if let Some(ref h) = handler {
                                            h.call(click_ref.clone());
                                        }
                                    },
                                    span { class: "card-icon", "{aref.icon}" }
                                    div {
                                        class: "card-info",
                                        div { class: "card-name", "{aref.name}" }
                                        div { class: "card-type", "{aref.artifact_type}" }
                                    }
                                }
                            }
                        }
                    }
                    // Image attachment
                    if msg.image_ref.is_some() {
                        div {
                            class: "msg-image",
                            div { class: "msg-image-placeholder", "\u{1F5FA}" }
                        }
                    }
                    // Branch indicator (inside bubble, matching HTML reference)
                    if let Some(branch) = &msg.branch_label {
                        div {
                            class: "msg-branch",
                            "\u{21AA} {branch}"
                        }
                    }
                }
                div { class: "msg-time", "{msg.time}" }
            }
        }
    }
}
