//! File preview panel showing rendered markdown content.

use dioxus::prelude::*;

use crate::state::AppState;

/// Right panel showing rendered HTML of the selected file.
/// Reads file content directly when selected_file changes.
#[component]
pub fn FilePreview(state: Signal<AppState>) -> Element {
    let selected = state.read().selected_file.clone();
    let vault_path = state.read().vault_path.clone();

    // Read and render content inline — reactive to selected_file changes
    let content = selected.as_ref().and_then(|name| {
        let full_path = vault_path.join(name);
        let raw = std::fs::read_to_string(&full_path).ok()?;
        Some(indras_ui::render_markdown_to_html(&raw))
    });

    rsx! {
        div { class: "file-preview",
            if let Some(ref name) = selected {
                div { class: "preview-header", "{name}" }
                if let Some(ref html) = content {
                    div {
                        class: "preview-body",
                        dangerous_inner_html: "{html}",
                    }
                } else {
                    div { class: "preview-loading", "Loading..." }
                }
            } else {
                div { class: "preview-empty",
                    div { class: "preview-empty-icon", "\u{1f4c4}" }
                    div { "Select a file to preview" }
                }
            }
        }
    }
}
