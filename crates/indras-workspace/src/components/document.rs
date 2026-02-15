use dioxus::prelude::*;
use dioxus::prelude::Key;
use crate::state::editor::{Block, EditorState};
use crate::state::workspace::WorkspaceState;
use crate::bridge::vault_bridge::VaultHandle;
use indras_artifacts::{Artifact, LeafType};
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
pub fn DocumentView(
    editor: EditorState,
    vault_handle: Signal<Option<VaultHandle>>,
    workspace: Signal<WorkspaceState>,
) -> Element {
    let mut editing_index: Signal<Option<usize>> = use_signal(|| None);
    let mut draft_content: Signal<String> = use_signal(String::new);

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
                    for (index, block) in editor.blocks.iter().enumerate() {
                        {
                            let is_editing = *editing_index.read() == Some(index);
                            if is_editing {
                                let draft_val = draft_content.read().clone();
                                let rows_str = draft_val.lines().count().max(1).to_string();

                                // Compute per-block-type editing class
                                let block_type_class = match block {
                                    Block::Heading { level, .. } => format!("block-editing-h{}", level),
                                    Block::Code { .. } => "block-editing-code".to_string(),
                                    Block::Callout { .. } => "block-editing-callout".to_string(),
                                    Block::Todo { .. } => "block-editing-todo".to_string(),
                                    _ => "block-editing-text".to_string(),
                                };
                                let block_class = format!("block block-edit-active {}", block_type_class);

                                let is_todo = matches!(block, Block::Todo { .. });
                                let todo_done = matches!(block, Block::Todo { done: true, .. });
                                let check_class = if todo_done { "todo-check done" } else { "todo-check" };
                                let check_mark = if todo_done { "\u{2713}" } else { "" };

                                if is_todo {
                                    rsx! {
                                        div {
                                            class: "{block_class}",
                                            div {
                                                class: "block-todo",
                                                div { class: "{check_class}", "{check_mark}" }
                                                textarea {
                                                    class: "block-edit-inline",
                                                    value: "{draft_val}",
                                                    autofocus: true,
                                                    rows: "{rows_str}",
                                                    oninput: move |evt: Event<FormData>| {
                                                        draft_content.set(evt.value());
                                                    },
                                                    onkeydown: move |evt: KeyboardEvent| {
                                                        handle_edit_keydown(
                                                            evt, index, editing_index,
                                                            draft_content, vault_handle, workspace,
                                                        );
                                                    },
                                                }
                                            }
                                            div { class: "block-edit-hint", "\u{23CE} Ctrl+Enter save \u{00B7} Esc cancel" }
                                        }
                                    }
                                } else {
                                    rsx! {
                                        div {
                                            class: "{block_class}",
                                            textarea {
                                                class: "block-edit-inline",
                                                value: "{draft_val}",
                                                autofocus: true,
                                                rows: "{rows_str}",
                                                oninput: move |evt: Event<FormData>| {
                                                    draft_content.set(evt.value());
                                                },
                                                onkeydown: move |evt: KeyboardEvent| {
                                                    handle_edit_keydown(
                                                        evt, index, editing_index,
                                                        draft_content, vault_handle, workspace,
                                                    );
                                                },
                                            }
                                            div { class: "block-edit-hint", "\u{23CE} Ctrl+Enter save \u{00B7} Esc cancel" }
                                        }
                                    }
                                }
                            } else if block.is_editable() {
                                let content_for_click = block.content().to_string();
                                rsx! {
                                    div {
                                        class: "block-clickable",
                                        onclick: move |_| {
                                            editing_index.set(Some(index));
                                            draft_content.set(content_for_click.clone());
                                        },
                                        {render_block(block)}
                                    }
                                }
                            } else {
                                render_block(block)
                            }
                        }
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

/// Handle keydown events for the inline editor textarea.
fn handle_edit_keydown(
    evt: KeyboardEvent,
    index: usize,
    mut editing_index: Signal<Option<usize>>,
    mut draft_content: Signal<String>,
    vault_handle: Signal<Option<VaultHandle>>,
    workspace: Signal<WorkspaceState>,
) {
    evt.stop_propagation();
    let key = evt.key();
    let mods = evt.modifiers();
    let ctrl_or_meta = mods.contains(Modifiers::CONTROL) || mods.contains(Modifiers::META);
    match key {
        Key::Enter if ctrl_or_meta => {
            let content = draft_content.read().clone();
            let tree_id = workspace.read().editor.tree_id.clone();
            spawn(async move {
                save_block(index, content, tree_id, vault_handle, workspace).await;
                editing_index.set(None);
                draft_content.set(String::new());
            });
        }
        Key::Escape => {
            editing_index.set(None);
            draft_content.set(String::new());
        }
        _ => {}
    }
}

/// Perform the vault write for an edited block.
async fn save_block(
    index: usize,
    new_content: String,
    tree_id: Option<indras_artifacts::ArtifactId>,
    vault_handle: Signal<Option<VaultHandle>>,
    mut workspace: Signal<WorkspaceState>,
) {
    let vh = match vault_handle.read().clone() {
        Some(h) => h,
        None => return,
    };
    let tree_id = match tree_id {
        Some(id) => id,
        None => return,
    };

    let mut vault = vh.vault.lock().await;
    let now = chrono::Utc::now().timestamp_millis();

    // Get the old reference at this index from the tree
    let old_ref = if let Ok(Some(Artifact::Tree(tree))) = vault.get_artifact(&tree_id) {
        tree.references.get(index).cloned()
    } else {
        None
    };
    let old_ref = match old_ref {
        Some(r) => r,
        None => return,
    };

    // Place new leaf with updated content
    let new_leaf = match vault.place_leaf(new_content.as_bytes(), LeafType::Message, now) {
        Ok(l) => l,
        Err(e) => {
            tracing::error!("Failed to place leaf: {}", e);
            return;
        }
    };

    // Remove old reference
    if let Err(e) = vault.remove_ref(&tree_id, &old_ref.artifact_id) {
        tracing::error!("Failed to remove ref: {}", e);
        return;
    }

    // Compose new reference at same position with same label
    if let Err(e) = vault.compose(&tree_id, new_leaf.id.clone(), old_ref.position, old_ref.label) {
        tracing::error!("Failed to compose ref: {}", e);
        return;
    }

    // Reload all blocks from the updated tree
    if let Ok(Some(Artifact::Tree(tree))) = vault.get_artifact(&tree_id) {
        let mut blocks = Vec::new();
        for child_ref in &tree.references {
            let content = if let Ok(Some(payload)) = vault.get_payload(&child_ref.artifact_id) {
                String::from_utf8_lossy(&payload).to_string()
            } else {
                String::new()
            };
            let block = EditorState::parse_block_from_label(
                &child_ref.label,
                content,
                Some(format!("{:?}", child_ref.artifact_id)),
            );
            blocks.push(block);
        }
        workspace.write().editor.blocks = blocks;
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
