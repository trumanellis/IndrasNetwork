//! Reusable file row component for vault columns.

use std::path::PathBuf;

use dioxus::prelude::*;

use crate::state::{DragPayload, FileView, RealmId};

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
///
/// When `file_disk_path` is provided, the row becomes draggable for
/// drag-to-share operations across columns.
#[component]
pub fn FileItem(
    file: FileView,
    is_selected: bool,
    on_click: EventHandler<String>,
    on_context_menu: Option<EventHandler<(String, f64, f64)>>,
    /// Full path to the file on disk (enables drag when Some).
    #[props(default)]
    file_disk_path: Option<PathBuf>,
    /// Which realm this file belongs to (None = private vault).
    #[props(default)]
    source_realm: Option<RealmId>,
    /// Fired when drag begins with the drag payload.
    #[props(default)]
    on_drag_start: Option<EventHandler<DragPayload>>,
    /// Fired when drag ends (drop or cancel).
    #[props(default)]
    on_drag_end: Option<EventHandler<()>>,
) -> Element {
    let class = if is_selected { "file-item selected" } else { "file-item" };
    let path = file.path.clone();
    let ctx_path = file.path.clone();
    let icon = file_icon(&file.name);
    let is_draggable = file_disk_path.is_some();

    // Capture values for drag closures
    let drag_name = file.name.clone();
    let drag_disk_path = file_disk_path.clone();
    let drag_source = source_realm;

    rsx! {
        div {
            class: "{class}",
            draggable: if is_draggable { "true" } else { "false" },
            onclick: move |_| on_click.call(path.clone()),
            oncontextmenu: move |e: MouseEvent| {
                e.prevent_default();
                if let Some(handler) = &on_context_menu {
                    let coords = e.page_coordinates();
                    handler.call((ctx_path.clone(), coords.x, coords.y));
                }
            },
            ondragstart: move |_evt| {
                if let (Some(disk_path), Some(handler)) = (&drag_disk_path, &on_drag_start) {
                    handler.call(DragPayload {
                        file_name: drag_name.clone(),
                        file_disk_path: disk_path.clone(),
                        source_realm: drag_source,
                    });
                }
            },
            ondragend: move |_evt| {
                if let Some(handler) = &on_drag_end {
                    handler.call(());
                }
            },
            span { class: "file-item-icon", "{icon}" }
            div { class: "file-item-content",
                div { class: "file-item-name", "{file.name}" }
                div { class: "file-item-meta", "{file.modified}" }
            }
        }
    }
}
