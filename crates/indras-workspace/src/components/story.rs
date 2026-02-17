//! Story (conversation/chat) view component.

use dioxus::prelude::*;
use indras_network::MessageId;

/// Embedded artifact reference in a story message.
#[derive(Clone, Debug, PartialEq)]
pub struct StoryArtifactRef {
    pub icon: String,
    pub name: String,
    pub artifact_type: String,
    /// Artifact ID for download (None for local-only artifacts).
    pub artifact_id: Option<String>,
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
    /// Realm message ID for reply/react (None for vault-only messages)
    pub message_id: Option<MessageId>,
    /// Emoji reactions on this message: (emoji, count)
    pub reactions: Vec<(String, usize)>,
    /// Quoted reply preview text (if this message is a reply)
    pub reply_to_preview: Option<String>,
}

/// Reply context for the compose bar.
#[derive(Clone, Debug, PartialEq)]
struct ReplyContext {
    message_id: MessageId,
    sender_name: String,
    preview: String,
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
    #[props(optional)]
    on_reply: Option<EventHandler<(MessageId, String)>>,
    #[props(optional)]
    on_react: Option<EventHandler<(MessageId, String)>>,
    #[props(optional)]
    on_search: Option<EventHandler<String>>,
    #[props(optional)]
    on_attach: Option<EventHandler<()>>,
) -> Element {
    let audience_text = format!("{} audience", audience_count);
    let msg_text = format!("{} messages", message_count);
    let mut compose_text = use_signal(String::new);
    let mut replying_to = use_signal(|| None::<ReplyContext>);
    let mut search_open = use_signal(|| false);
    let mut search_text = use_signal(String::new);

    rsx! {
        div {
            class: "view active",
            div {
                class: "story-scroll",
                div {
                    class: "story-header",
                    div {
                        class: "story-header-row",
                        div { class: "story-title", "{title}" }
                        button {
                            class: "story-search-toggle",
                            onclick: move |_| {
                                let is_open = *search_open.read();
                                search_open.set(!is_open);
                                if is_open {
                                    // Closing search — clear and reload all messages
                                    search_text.set(String::new());
                                    if let Some(handler) = &on_search {
                                        handler.call(String::new());
                                    }
                                }
                            },
                            "\u{1F50D}"
                        }
                    }
                    if *search_open.read() {
                        div {
                            class: "story-search-bar",
                            input {
                                class: "story-search-input",
                                placeholder: "Search messages...",
                                value: "{search_text}",
                                oninput: move |evt| search_text.set(evt.value()),
                                onkeydown: move |evt: KeyboardEvent| {
                                    if evt.key() == Key::Enter {
                                        let query = search_text.read().trim().to_string();
                                        if let Some(handler) = &on_search {
                                            handler.call(query);
                                        }
                                    }
                                },
                            }
                        }
                    }
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
                        {render_message(msg, &on_artifact_click, &on_react, &replying_to)}
                    }
                }
            }
            // Reply quote bar (shown when replying)
            if let Some(ctx) = replying_to.read().as_ref() {
                div {
                    class: "compose-reply-bar",
                    span { class: "compose-reply-label", "Replying to {ctx.sender_name}" }
                    span { class: "compose-reply-preview", "{ctx.preview}" }
                    button {
                        class: "compose-reply-cancel",
                        onclick: move |_| replying_to.set(None),
                        "\u{2715}"
                    }
                }
            }
            // Compose bar
            div {
                class: "compose-bar",
                button {
                    class: "compose-attach",
                    onclick: move |_| {
                        if let Some(handler) = &on_attach {
                            handler.call(());
                        }
                    },
                    "\u{1F4CE}"
                }
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
                            let reply_ctx = replying_to.read().clone();
                            if let Some(ctx) = reply_ctx {
                                if let Some(handler) = &on_reply {
                                    handler.call((ctx.message_id, text));
                                }
                                replying_to.set(None);
                            } else if let Some(handler) = &on_send {
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

fn render_message(
    msg: &StoryMessage,
    on_artifact_click: &Option<EventHandler<StoryArtifactRef>>,
    on_react: &Option<EventHandler<(MessageId, String)>>,
    replying_to: &Signal<Option<ReplyContext>>,
) -> Element {
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

    // Capture for reply/react closures
    let msg_id = msg.message_id;
    let sender_for_reply = msg.sender_name.clone();
    let preview_for_reply = msg.content.chars().take(60).collect::<String>();
    let mut replying_to = *replying_to;

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
                // Reply quote (if this message is a reply)
                if let Some(ref preview) = msg.reply_to_preview {
                    div {
                        class: "msg-reply-quote",
                        "\u{21A9} {preview}"
                    }
                }
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
                // Reactions display
                if !msg.reactions.is_empty() {
                    div {
                        class: "msg-reactions",
                        for (emoji, count) in msg.reactions.iter() {
                            span {
                                class: "msg-reaction-badge",
                                "{emoji} {count}"
                            }
                        }
                    }
                }
                div {
                    class: "msg-footer",
                    div { class: "msg-time", "{msg.time}" }
                    // Reply and react actions (only for realm messages)
                    if msg_id.is_some() {
                        div {
                            class: "msg-actions",
                            button {
                                class: "msg-action-btn",
                                title: "Reply",
                                onclick: move |_| {
                                    if let Some(id) = msg_id {
                                        replying_to.set(Some(ReplyContext {
                                            message_id: id,
                                            sender_name: sender_for_reply.clone(),
                                            preview: preview_for_reply.clone(),
                                        }));
                                    }
                                },
                                "\u{21A9}"
                            }
                            {
                                let react_handler = on_react.clone();
                                let emojis = ["\u{1F44D}", "\u{2764}", "\u{1F604}", "\u{1F914}"];
                                rsx! {
                                    for emoji in emojis.iter() {
                                        {
                                            let emoji_str = emoji.to_string();
                                            let handler = react_handler.clone();
                                            rsx! {
                                                button {
                                                    class: "msg-action-btn msg-react-btn",
                                                    onclick: move |_| {
                                                        if let (Some(id), Some(h)) = (msg_id, &handler) {
                                                            h.call((id, emoji_str.clone()));
                                                        }
                                                    },
                                                    "{emoji}"
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
        }
    }
}
