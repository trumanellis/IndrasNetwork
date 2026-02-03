//! File preview overlay components.

use dioxus::prelude::*;

use crate::markdown::{is_markdown_file, render_markdown_to_html};

/// File being previewed in overlay.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PreviewFile {
    pub name: String,
    /// Content with artifact references resolved to data URLs (for rendered mode).
    pub content: String,
    /// Content with artifact references resolved to friendly filenames (for raw mode).
    pub raw_content: String,
    pub mime_type: String,
    /// Data URL for image preview (set when previewing an image file).
    pub data_url: Option<String>,
}

/// View mode for markdown preview.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum PreviewViewMode {
    #[default]
    Rendered,
    Raw,
}

/// Context for markdown preview overlay.
#[derive(Clone, Copy)]
pub struct PreviewContext {
    pub is_open: Signal<bool>,
    pub file: Signal<Option<PreviewFile>>,
    pub view_mode: Signal<PreviewViewMode>,
}

/// File preview overlay -- handles markdown (rendered/raw) and images.
#[component]
pub fn MarkdownPreviewOverlay(
    is_open: Signal<bool>,
    file: Signal<Option<PreviewFile>>,
    view_mode: Signal<PreviewViewMode>,
) -> Element {
    if !is_open() {
        return rsx! {};
    }
    let Some(file_data) = file() else {
        return rsx! {};
    };

    let is_image = file_data.mime_type.starts_with("image/");
    let is_md = !is_image && is_markdown_file(&file_data.name, &file_data.mime_type);
    let mode = view_mode();

    let rendered_html = if is_md && mode == PreviewViewMode::Rendered {
        Some(render_markdown_to_html(&file_data.content))
    } else {
        None
    };

    rsx! {
        div {
            class: "markdown-preview-overlay",
            onclick: move |_| is_open.set(false),
            div {
                class: "markdown-preview-dialog",
                onclick: move |e| e.stop_propagation(),

                // Header
                div { class: "markdown-preview-header",
                    span { class: "markdown-preview-filename", "{file_data.name}" }
                    div { class: "markdown-preview-controls",
                        if is_md {
                            button {
                                class: "markdown-preview-toggle",
                                onclick: move |_| {
                                    view_mode.set(if mode == PreviewViewMode::Rendered {
                                        PreviewViewMode::Raw
                                    } else {
                                        PreviewViewMode::Rendered
                                    });
                                },
                                if mode == PreviewViewMode::Rendered { "View Raw" } else { "View Rendered" }
                            }
                        }
                        button {
                            class: "markdown-preview-close",
                            onclick: move |_| is_open.set(false),
                            "\u{00d7}"
                        }
                    }
                }

                // Content
                div { class: "markdown-preview-content",
                    if is_image {
                        if let Some(ref url) = file_data.data_url {
                            div { class: "image-preview",
                                img {
                                    class: "image-preview-img",
                                    src: "{url}",
                                    alt: "{file_data.name}",
                                }
                            }
                        }
                    } else if let Some(ref html) = rendered_html {
                        div { class: "markdown-rendered", dangerous_inner_html: "{html}" }
                    } else {
                        pre { class: "markdown-raw", "{file_data.raw_content}" }
                    }
                }
            }
        }
    }
}
