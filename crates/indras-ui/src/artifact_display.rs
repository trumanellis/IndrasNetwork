//! Shared presentational types and components for artifact display.
//!
//! These are pure view-model types with no dependency on `indras-network`.
//! Consumers map their domain artifacts into `ArtifactDisplayInfo` and render
//! them with `ArtifactGallery`.

use dioxus::prelude::*;

/// Display status of an artifact (presentational mirror of network status).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum ArtifactDisplayStatus {
    #[default]
    Active,
    Recalled,
    Transferred,
    Expired,
}

impl ArtifactDisplayStatus {
    pub fn css_class(&self) -> &'static str {
        match self {
            Self::Active => "artifact-status-active",
            Self::Recalled => "artifact-status-recalled",
            Self::Transferred => "artifact-status-transferred",
            Self::Expired => "artifact-status-expired",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Active => "Active",
            Self::Recalled => "Recalled",
            Self::Transferred => "Transferred",
            Self::Expired => "Expired",
        }
    }
}

/// Presentational view model for a single artifact.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ArtifactDisplayInfo {
    pub id: String,
    pub name: String,
    pub size: u64,
    pub mime_type: Option<String>,
    pub status: ArtifactDisplayStatus,
    /// Pre-computed image data URL for thumbnails.
    pub data_url: Option<String>,
    pub grant_count: usize,
    /// "Private" or "Shared with N".
    pub owner_label: Option<String>,
}

impl ArtifactDisplayInfo {
    /// Emoji icon based on mime type or file extension.
    pub fn icon(&self) -> &'static str {
        if let Some(ref mime) = self.mime_type {
            if mime.starts_with("image/") {
                return "\u{1f5bc}\u{fe0f}";
            } else if mime.starts_with("video/") {
                return "\u{1f3ac}";
            } else if mime.starts_with("audio/") {
                return "\u{1f3b5}";
            } else if mime.starts_with("text/") {
                return "\u{1f4c4}";
            } else if mime == "application/pdf" {
                return "\u{1f4d5}";
            }
        }
        if let Some(ext) = self.name.rsplit('.').next() {
            match ext.to_lowercase().as_str() {
                "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" => "\u{1f5bc}\u{fe0f}",
                "mp4" | "mov" | "avi" | "mkv" | "webm" => "\u{1f3ac}",
                "mp3" | "wav" | "flac" | "ogg" | "m4a" => "\u{1f3b5}",
                "pdf" => "\u{1f4d5}",
                "doc" | "docx" => "\u{1f4d8}",
                "xls" | "xlsx" => "\u{1f4d7}",
                "zip" | "tar" | "gz" | "rar" => "\u{1f4e6}",
                "rs" | "py" | "js" | "ts" | "lua" => "\u{1f4bb}",
                _ => "\u{1f4c4}",
            }
        } else {
            "\u{1f4c4}"
        }
    }

    /// Human-readable file size.
    pub fn formatted_size(&self) -> String {
        if self.size < 1024 {
            format!("{} B", self.size)
        } else if self.size < 1024 * 1024 {
            format!("{:.1} KB", self.size as f64 / 1024.0)
        } else if self.size < 1024 * 1024 * 1024 {
            format!("{:.1} MB", self.size as f64 / (1024.0 * 1024.0))
        } else {
            format!("{:.2} GB", self.size as f64 / (1024.0 * 1024.0 * 1024.0))
        }
    }

    /// Whether this artifact has a displayable image (by mime or extension).
    pub fn has_displayable_image(&self) -> bool {
        if let Some(ref mime) = self.mime_type {
            if mime.starts_with("image/") {
                return true;
            }
        }
        if let Some(ext) = self.name.rsplit('.').next() {
            matches!(
                ext.to_lowercase().as_str(),
                "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg"
            )
        } else {
            false
        }
    }

    /// Whether this artifact is an image type.
    pub fn is_image(&self) -> bool {
        self.has_displayable_image()
    }
}

/// A responsive grid gallery of artifact cards.
#[component]
pub fn ArtifactGallery(artifacts: Vec<ArtifactDisplayInfo>) -> Element {
    rsx! {
        div {
            class: "artifact-gallery-grid",
            for artifact in artifacts.iter() {
                ArtifactCard { key: "{artifact.id}", artifact: artifact.clone() }
            }
        }
    }
}

/// A single artifact card in the gallery grid.
#[component]
fn ArtifactCard(artifact: ArtifactDisplayInfo) -> Element {
    let icon = artifact.icon();
    let size_str = artifact.formatted_size();
    let status_class = artifact.status.css_class();
    let status_label = artifact.status.label();
    let owner_label = artifact.owner_label.clone().unwrap_or_default();
    let has_image = artifact.has_displayable_image() && artifact.data_url.is_some();

    rsx! {
        div {
            class: "artifact-gallery-card",

            // Thumbnail area
            div {
                class: "artifact-gallery-thumb",
                if has_image {
                    if let Some(ref url) = artifact.data_url {
                        img {
                            class: "artifact-gallery-thumb-img",
                            src: "{url}",
                            alt: "{artifact.name}",
                        }
                    }
                } else {
                    span { class: "artifact-gallery-icon", "{icon}" }
                }
            }

            // Info area
            div {
                class: "artifact-gallery-info",
                div { class: "artifact-gallery-name", "{artifact.name}" }
                div {
                    class: "artifact-gallery-meta",
                    span { "{size_str}" }
                    if !owner_label.is_empty() {
                        span { " \u{b7} {owner_label}" }
                    }
                }
                span {
                    class: "artifact-gallery-status {status_class}",
                    "{status_label}"
                }
            }
        }
    }
}
