//! WYSIWYG markdown editor powered by Milkdown.
//!
//! Embeds the Milkdown JS editor (ProseMirror-based) in the Dioxus webview.
//! Content auto-saves to disk on every change (debounced). The Rust side
//! communicates with JS via `document::eval()` and `dioxus.send()`.

use std::path::PathBuf;

use dioxus::prelude::*;

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

/// Escape a string for embedding in a JavaScript single-quoted string literal.
fn escape_for_js(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\'' => out.push_str("\\'"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out.push('\'');
    out
}

/// WYSIWYG Milkdown markdown editor: renders as a rich editor, auto-saves to
/// disk on content changes.
///
/// `on_content`, if provided, is invoked with the full markdown after every
/// persisted write (both the polling tick and the flush-on-unmount). It lets
/// callers mirror content into a CRDT or other side-channel without blocking
/// the editor's own disk write.
#[component]
pub fn InlineMarkdownEditor(
    full_path: PathBuf,
    on_content: Option<Callback<String>>,
) -> Element {
    let mut loaded_path: Signal<Option<PathBuf>> = use_signal(|| None);
    // Generation counter: bumped on each file switch so stale save loops
    // silently stop writing to the wrong file.
    let mut generation: Signal<u64> = use_signal(|| 0);
    // Track the current file path for flush-on-unmount.
    let mut save_path: Signal<PathBuf> = use_signal(|| full_path.clone());

    // Flush pending Milkdown content to disk when the component unmounts
    // (e.g. modal close). getMarkdown() falls back to _milkdownContent if
    // the editor DOM is already gone, so this is safe even in a race.
    use_drop({
        move || {
            let fp = save_path.read().clone();
            let cb = on_content;
            spawn(async move {
                let mut eval =
                    document::eval("dioxus.send(window.MilkdownBridge.getMarkdown())");
                if let Ok(md) = eval.recv::<String>().await {
                    let _ = std::fs::write(&fp, &md);
                    if let Some(cb) = cb {
                        cb.call(md);
                    }
                }
            });
        }
    });

    let needs_init = loaded_path.read().as_ref() != Some(&full_path);

    if needs_init {
        let cur_gen = *generation.read() + 1;
        generation.set(cur_gen);
        loaded_path.set(Some(full_path.clone()));
        save_path.set(full_path.clone());

        let content = std::fs::read_to_string(&full_path).unwrap_or_default();
        let escaped = escape_for_js(&content);
        let fp = full_path.clone();

        spawn(async move {
            // Brief delay so the container div is in the DOM
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;

            // Initialize Milkdown in the container
            let init_js = format!(
                "await window.MilkdownBridge.init('milkdown-editor', {})",
                escaped
            );
            let _ = document::eval(&init_js);

            // Another brief delay for editor to fully mount
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;

            // Start a polling loop that picks up dirty content and sends it
            // back to Rust for disk persistence.
            let mut save_eval = document::eval(
                r#"
                const _interval = setInterval(() => {
                    if (window._milkdownDirty) {
                        dioxus.send(window._milkdownContent);
                        window._milkdownDirty = false;
                    }
                }, 500);
                "#,
            );

            loop {
                match save_eval.recv::<String>().await {
                    Ok(md) => {
                        // If generation changed, a new file was loaded and
                        // a new save loop is running — stop this one.
                        if *generation.read() != cur_gen {
                            break;
                        }
                        let _ = std::fs::write(&fp, &md);
                        if let Some(cb) = on_content {
                            cb.call(md);
                        }
                    }
                    Err(_) => break,
                }
            }
        });
    }

    rsx! {
        div {
            id: "milkdown-editor",
            class: "milkdown-container",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn js_escape_basic() {
        assert_eq!(escape_for_js("hello"), "'hello'");
    }

    #[test]
    fn js_escape_special_chars() {
        assert_eq!(escape_for_js("a'b\\c"), "'a\\'b\\\\c'");
    }

    #[test]
    fn js_escape_newlines() {
        assert_eq!(escape_for_js("line1\nline2"), "'line1\\nline2'");
    }

    #[test]
    fn obsidian_url() {
        let p = std::path::Path::new("/Users/test/vault/note.md");
        let url = obsidian_open_url(p);
        assert!(url.starts_with("obsidian://open?path="));
        assert!(url.contains("note.md"));
    }
}
