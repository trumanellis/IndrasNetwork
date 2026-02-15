use dioxus::prelude::*;
use crate::state::editor::{Block, EditorState};
use super::blocks::{
    text::TextBlock,
    heading::HeadingBlock,
    code::CodeBlock,
    callout::CalloutBlock,
    todo::TodoBlock,
    image::ImageBlock,
    divider::DividerBlock,
};

#[component]
pub fn DocumentView(editor: EditorState) -> Element {
    let type_class = format!("type-{}", editor.meta.doc_type.to_lowercase());
    let audience_text = format!("{} audience", editor.meta.audience_count);

    rsx! {
        div {
            class: "view active",
            div {
                class: "content-scroll",
                div {
                    class: "content-body",
                    div { class: "doc-title", "{editor.title}" }
                    div {
                        class: "doc-meta",
                        span { class: "doc-meta-tag {type_class}", "{editor.meta.doc_type}" }
                        span { "{audience_text}" }
                        if !editor.meta.edited_ago.is_empty() {
                            span { "\u{00B7}" }
                            span { "Edited {editor.meta.edited_ago}" }
                        }
                    }
                    for block in editor.blocks.iter() {
                        {render_block(block)}
                    }
                    div {
                        class: "block-placeholder",
                        "Type / for commands..."
                    }
                }
            }
        }
    }
}

fn render_block(block: &Block) -> Element {
    match block {
        Block::Text { content, .. } => rsx! {
            TextBlock { content: content.clone() }
        },
        Block::Heading { level, content, .. } => rsx! {
            HeadingBlock { level: *level, content: content.clone() }
        },
        Block::Code { language, content, .. } => rsx! {
            CodeBlock { content: content.clone(), language: language.clone() }
        },
        Block::Callout { content, .. } => rsx! {
            CalloutBlock { content: content.clone() }
        },
        Block::Todo { text, done, .. } => rsx! {
            TodoBlock { text: text.clone(), done: *done }
        },
        Block::Image { caption, .. } => rsx! {
            ImageBlock { caption: caption.clone() }
        },
        Block::Divider => rsx! {
            DividerBlock {}
        },
        Block::Placeholder => rsx! {
            div {
                class: "block-placeholder",
                "Type / for commands..."
            }
        },
    }
}
