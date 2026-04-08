//! Reusable file row component for vault columns.

use dioxus::prelude::*;

use crate::state::FileView;

/// Returns an emoji icon for the given filename based on its extension.
fn file_icon(name: &str) -> &'static str {
    let lower = name.to_lowercase();
    if lower.ends_with(".md") || lower.ends_with(".markdown") {
        "📝"
    } else if lower.ends_with(".png")
        || lower.ends_with(".jpg")
        || lower.ends_with(".jpeg")
        || lower.ends_with(".gif")
        || lower.ends_with(".svg")
    {
        "🖼"
    } else if lower.ends_with(".pdf") {
        "📄"
    } else {
        "📎"
    }
}

/// A single file row showing a type icon, name, and modification time.
#[component]
pub fn FileItem(
    file: FileView,
    is_selected: bool,
    on_click: EventHandler<String>,
) -> Element {
    let class = if is_selected { "file-item selected" } else { "file-item" };
    let path = file.path.clone();
    let icon = file_icon(&file.name);

    rsx! {
        div {
            class: "{class}",
            onclick: move |_| on_click.call(path.clone()),
            span { class: "file-item-icon", "{icon}" }
            div { class: "file-item-content",
                div { class: "file-item-name", "{file.name}" }
                div { class: "file-item-meta", "{file.modified}" }
            }
        }
    }
}
