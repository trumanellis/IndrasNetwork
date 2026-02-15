//! Slash command menu for creating new blocks and artifacts.

use dioxus::prelude::*;

/// Action types available in the slash menu.
#[derive(Clone, Debug, PartialEq)]
pub enum SlashAction {
    // Leaf blocks
    Text,
    Heading,
    Code,
    Callout,
    Todo,
    Image,
    Divider,
    // Tree types
    Document,
    Story,
    Quest,
    Need,
    Offering,
    Intention,
}

impl SlashAction {
    pub fn label(&self) -> &str {
        match self {
            SlashAction::Text => "Text",
            SlashAction::Heading => "Heading",
            SlashAction::Code => "Code Block",
            SlashAction::Callout => "Callout",
            SlashAction::Todo => "To-do",
            SlashAction::Image => "Image",
            SlashAction::Divider => "Divider",
            SlashAction::Document => "Document",
            SlashAction::Story => "Story",
            SlashAction::Quest => "Quest",
            SlashAction::Need => "Need",
            SlashAction::Offering => "Offering",
            SlashAction::Intention => "Intention",
        }
    }

    pub fn description(&self) -> &str {
        match self {
            SlashAction::Text => "Plain text paragraph",
            SlashAction::Heading => "Section heading",
            SlashAction::Code => "Syntax-highlighted code",
            SlashAction::Callout => "Highlighted callout box",
            SlashAction::Todo => "Checkbox task item",
            SlashAction::Image => "Image from artifact",
            SlashAction::Divider => "Horizontal separator",
            SlashAction::Document => "New document tree",
            SlashAction::Story => "New conversation thread",
            SlashAction::Quest => "Call to action",
            SlashAction::Need => "Request for help",
            SlashAction::Offering => "Gift of service",
            SlashAction::Intention => "Personal aspiration",
        }
    }

    pub fn icon(&self) -> &str {
        match self {
            SlashAction::Text => "📝",
            SlashAction::Heading => "H",
            SlashAction::Code => "</>",
            SlashAction::Callout => "💡",
            SlashAction::Todo => "☑",
            SlashAction::Image => "🖼",
            SlashAction::Divider => "—",
            SlashAction::Document => "📄",
            SlashAction::Story => "💬",
            SlashAction::Quest => "⚔",
            SlashAction::Need => "🌱",
            SlashAction::Offering => "🎁",
            SlashAction::Intention => "✨",
        }
    }
}

/// Slash command menu overlay.
#[component]
pub fn SlashMenu(
    visible: bool,
    on_select: EventHandler<SlashAction>,
    on_close: EventHandler<()>,
) -> Element {
    if !visible {
        return rsx! {};
    }

    let leaf_items = vec![
        SlashAction::Text,
        SlashAction::Heading,
        SlashAction::Code,
        SlashAction::Callout,
        SlashAction::Todo,
        SlashAction::Image,
        SlashAction::Divider,
    ];

    let tree_items = vec![
        SlashAction::Document,
        SlashAction::Story,
        SlashAction::Quest,
        SlashAction::Need,
        SlashAction::Offering,
        SlashAction::Intention,
    ];

    rsx! {
        div {
            class: "slash-menu visible",
            div { class: "slash-menu-title", "Leaf Blocks" }
            for item in leaf_items.iter() {
                {
                    let action = item.clone();
                    rsx! {
                        div {
                            class: "slash-item",
                            onclick: move |_| on_select.call(action.clone()),
                            div { class: "slash-item-icon", "{item.icon()}" }
                            div {
                                div { class: "slash-item-name", "{item.label()}" }
                                div { class: "slash-item-desc", "{item.description()}" }
                            }
                        }
                    }
                }
            }
            div { class: "slash-menu-title", "Tree Artifacts" }
            for item in tree_items.iter() {
                {
                    let action = item.clone();
                    rsx! {
                        div {
                            class: "slash-item",
                            onclick: move |_| on_select.call(action.clone()),
                            div { class: "slash-item-icon", "{item.icon()}" }
                            div {
                                div { class: "slash-item-name", "{item.label()}" }
                                div { class: "slash-item-desc", "{item.description()}" }
                            }
                        }
                    }
                }
            }
        }
    }
}
