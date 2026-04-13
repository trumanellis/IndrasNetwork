//! Inline block-based markdown editor.
//!
//! The document is always shown as rendered markdown. Clicking any block
//! swaps it inline for an auto-sized textarea containing that block's raw
//! source. Blur, Escape, or Ctrl/Cmd+Enter commits and persists to disk.
//!
//! There is no "edit mode" toggle — rendered view and editing coexist on
//! a per-block basis so the user can write without ever leaving the page.

use std::path::PathBuf;

use dioxus::prelude::*;

/// Split a markdown source into coarse editable blocks.
///
/// Blocks are separated by blank lines; fenced code blocks (``` or ~~~) are
/// kept intact even if they contain blank lines. Each returned string has its
/// trailing newlines stripped — re-join with [`join_blocks`].
pub fn split_blocks(src: &str) -> Vec<String> {
    let mut blocks: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut in_fence = false;
    let mut fence_marker: &str = "```";

    let flush = |cur: &mut String, blocks: &mut Vec<String>| {
        if !cur.trim().is_empty() {
            blocks.push(cur.trim_end_matches('\n').to_string());
        }
        cur.clear();
    };

    for line in src.split_inclusive('\n') {
        let stripped = line.trim_end_matches('\n');
        let t = stripped.trim_start();

        if in_fence {
            current.push_str(line);
            if t.starts_with(fence_marker) {
                in_fence = false;
                flush(&mut current, &mut blocks);
            }
            continue;
        }

        if t.starts_with("```") || t.starts_with("~~~") {
            flush(&mut current, &mut blocks);
            in_fence = true;
            fence_marker = if t.starts_with("```") { "```" } else { "~~~" };
            current.push_str(line);
            continue;
        }

        if stripped.trim().is_empty() {
            flush(&mut current, &mut blocks);
            continue;
        }

        current.push_str(line);
    }

    flush(&mut current, &mut blocks);
    blocks
}

/// Percent-encode a filesystem path for use in an `obsidian://` URL.
/// Keeps unreserved chars and `/` literal; encodes everything else.
fn percent_encode_path(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for b in path.as_bytes() {
        let c = *b;
        let keep = c.is_ascii_alphanumeric()
            || matches!(c, b'-' | b'_' | b'.' | b'~' | b'/');
        if keep {
            out.push(c as char);
        } else {
            use std::fmt::Write;
            let _ = write!(out, "%{:02X}", c);
        }
    }
    out
}

/// Build the `obsidian://open?path=...` URL for a given absolute path.
pub fn obsidian_open_url(full_path: &std::path::Path) -> String {
    format!(
        "obsidian://open?path={}",
        percent_encode_path(&full_path.to_string_lossy())
    )
}

/// Classify a block's markdown source so we can apply matching typography
/// to the edit-mode textarea (otherwise a heading's raw source would render
/// in body font and cause a visual jump when the user clicks in).
fn block_type_class(src: &str) -> &'static str {
    let t = src.trim_start();
    if t.starts_with("# ") { return "md-block-h1"; }
    if t.starts_with("## ") { return "md-block-h2"; }
    if t.starts_with("### ") { return "md-block-h3"; }
    if t.starts_with("#### ") || t.starts_with("##### ") || t.starts_with("###### ") {
        return "md-block-h4";
    }
    if t.starts_with("```") || t.starts_with("~~~") { return "md-block-code"; }
    if t.starts_with("> ") { return "md-block-quote"; }
    "md-block-p"
}

/// Re-join blocks with blank-line separators so downstream markdown parsers
/// treat them as distinct paragraphs.
pub fn join_blocks(blocks: &[String]) -> String {
    let mut out = String::new();
    for (i, b) in blocks.iter().enumerate() {
        if i > 0 {
            out.push_str("\n\n");
        }
        out.push_str(b.trim_end_matches('\n'));
    }
    if !out.is_empty() {
        out.push('\n');
    }
    out
}

/// Zero-friction inline markdown editor: renders the file's markdown as a
/// stack of styled blocks. Click any block to edit it in place.
#[component]
pub fn InlineMarkdownEditor(full_path: PathBuf) -> Element {
    let mut blocks = use_signal(Vec::<String>::new);
    let mut loaded_path = use_signal(|| Option::<PathBuf>::None);
    let mut editing_index = use_signal(|| Option::<usize>::None);
    let mut draft = use_signal(String::new);

    // Reload blocks whenever the target file changes.
    if loaded_path.read().as_ref() != Some(&full_path) {
        let content = std::fs::read_to_string(&full_path).unwrap_or_default();
        blocks.set(split_blocks(&content));
        loaded_path.set(Some(full_path.clone()));
        editing_index.set(None);
        draft.set(String::new());
    }

    let persist = {
        let full_path = full_path.clone();
        move |blocks_signal: Signal<Vec<String>>| {
            let content = join_blocks(&blocks_signal.read());
            let _ = std::fs::write(&full_path, content);
        }
    };

    let current_editing = *editing_index.read();
    let block_count = blocks.read().len();

    rsx! {
        div { class: "md-editor",
            for index in 0..block_count {
                {
                    let source = blocks.read().get(index).cloned().unwrap_or_default();
                    let is_editing = current_editing == Some(index);
                    if is_editing {
                        let draft_val = draft.read().clone();
                        let persist = persist.clone();
                        let type_class = block_type_class(&draft_val);
                        rsx! {
                            div { class: "md-block md-block-editing {type_class}",
                                textarea {
                                    class: "md-block-input",
                                    value: "{draft_val}",
                                    autofocus: true,
                                    oninput: move |e| draft.set(e.value()),
                                    onblur: {
                                        let persist = persist.clone();
                                        move |_| {
                                            let new_src = draft.read().trim_end_matches('\n').to_string();
                                            let mut bs = blocks.write();
                                            if new_src.trim().is_empty() {
                                                if index < bs.len() {
                                                    bs.remove(index);
                                                }
                                            } else if index < bs.len() {
                                                bs[index] = new_src;
                                            }
                                            drop(bs);
                                            persist(blocks);
                                            editing_index.set(None);
                                            draft.set(String::new());
                                        }
                                    },
                                    onkeydown: move |e: KeyboardEvent| {
                                        let mods = e.modifiers();
                                        if (mods.meta() || mods.ctrl()) && e.key() == Key::Enter {
                                            e.prevent_default();
                                            // Blur will fire the save via onblur.
                                            editing_index.set(None);
                                        }
                                        if e.key() == Key::Escape {
                                            editing_index.set(None);
                                            draft.set(String::new());
                                        }
                                    },
                                }
                            }
                        }
                    } else {
                        let html = indras_ui::render_markdown_to_html(&source);
                        let src_for_click = source.clone();
                        let type_class = block_type_class(&source);
                        rsx! {
                            div {
                                class: "md-block md-block-rendered preview-body {type_class}",
                                onclick: move |_| {
                                    draft.set(src_for_click.clone());
                                    editing_index.set(Some(index));
                                },
                                div { dangerous_inner_html: "{html}" }
                            }
                        }
                    }
                }
            }

            // Ghost block — always-available append point.
            {
                let appending = current_editing == Some(block_count);
                if appending {
                    let draft_val = draft.read().clone();
                    let rows = draft_val.lines().count().max(1).to_string();
                    let persist = persist.clone();
                    let type_class = block_type_class(&draft_val);
                    rsx! {
                        div { class: "md-block md-block-editing {type_class}",
                            textarea {
                                class: "md-block-input",
                                value: "{draft_val}",
                                autofocus: true,
                                rows: "{rows}",
                                oninput: move |e| draft.set(e.value()),
                                onblur: {
                                    let persist = persist.clone();
                                    move |_| {
                                        let new_src = draft.read().trim_end_matches('\n').to_string();
                                        if !new_src.trim().is_empty() {
                                            blocks.write().push(new_src);
                                            persist(blocks);
                                        }
                                        editing_index.set(None);
                                        draft.set(String::new());
                                    }
                                },
                                onkeydown: move |e: KeyboardEvent| {
                                    let mods = e.modifiers();
                                    if (mods.meta() || mods.ctrl()) && e.key() == Key::Enter {
                                        e.prevent_default();
                                        editing_index.set(None);
                                    }
                                    if e.key() == Key::Escape {
                                        editing_index.set(None);
                                        draft.set(String::new());
                                    }
                                },
                            }
                        }
                    }
                } else {
                    let placeholder = if block_count == 0 {
                        "Click to start writing..."
                    } else {
                        "+ Add block"
                    };
                    rsx! {
                        div {
                            class: "md-block md-block-append",
                            onclick: move |_| {
                                draft.set(String::new());
                                editing_index.set(Some(block_count));
                            },
                            "{placeholder}"
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_on_blank_lines() {
        let src = "# Heading\n\nFirst paragraph.\n\nSecond paragraph.\n";
        let blocks = split_blocks(src);
        assert_eq!(blocks, vec!["# Heading", "First paragraph.", "Second paragraph."]);
    }

    #[test]
    fn preserves_fenced_code() {
        let src = "Intro.\n\n```rust\nfn main() {\n\n    println!(\"hi\");\n}\n```\n\nAfter.\n";
        let blocks = split_blocks(src);
        assert_eq!(blocks.len(), 3);
        assert!(blocks[1].starts_with("```rust"));
        assert!(blocks[1].contains("println"));
        assert!(blocks[1].ends_with("```"));
    }

    #[test]
    fn round_trip_join() {
        let src = "# A\n\nB\n\nC\n";
        let joined = join_blocks(&split_blocks(src));
        assert_eq!(joined, src);
    }
}
