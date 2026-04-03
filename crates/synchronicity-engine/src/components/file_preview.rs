//! File preview panel showing rendered markdown content.

use dioxus::prelude::*;

use crate::state::AppState;

/// Right panel showing rendered HTML of the selected file.
/// Shows an empty state when no file is selected.
#[component]
pub fn FilePreview(state: Signal<AppState>) -> Element {
    let content = state.read().selected_content.clone();
    let selected = state.read().selected_file.clone();

    rsx! {
        div { class: "file-preview",
            if let Some(name) = selected {
                div { class: "preview-header", "{name}" }
                if let Some(html) = content {
                    div {
                        class: "preview-body",
                        dangerous_inner_html: "{html}",
                    }
                } else {
                    div { class: "preview-loading", "Loading..." }
                }
            } else {
                div { class: "preview-empty",
                    div { class: "preview-empty-icon", "📄" }
                    div { "Select a file to preview" }
                }
            }
        }
    }
}
