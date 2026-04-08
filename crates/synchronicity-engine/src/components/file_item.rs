//! Reusable file row component for vault columns.

use dioxus::prelude::*;

use crate::state::FileView;

/// A single file row showing name and modification time.
#[component]
pub fn FileItem(
    file: FileView,
    is_selected: bool,
    on_click: EventHandler<String>,
) -> Element {
    let class = if is_selected { "file-item selected" } else { "file-item" };
    let path = file.path.clone();

    rsx! {
        div {
            class: "{class}",
            onclick: move |_| on_click.call(path.clone()),
            div { class: "file-item-name", "{file.name}" }
            div { class: "file-item-meta", "{file.modified}" }
        }
    }
}
