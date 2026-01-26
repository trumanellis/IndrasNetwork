//! State tracking for artifacts (uploaded files) in the home realm.

use std::collections::HashMap;

use crate::events::HomeRealmEvent;

/// An artifact (uploaded file) in the home realm.
#[derive(Debug, Clone, PartialEq)]
pub struct Artifact {
    pub id: String,
    pub size: u64,
    pub mime_type: String,
    pub uploaded_tick: u32,
    pub retrieved_count: u32,
}

impl Artifact {
    /// Creates a new artifact.
    pub fn new(id: String, size: u64, mime_type: String, uploaded_tick: u32) -> Self {
        Self {
            id,
            size,
            mime_type,
            uploaded_tick,
            retrieved_count: 0,
        }
    }

    /// Returns a display-friendly file type.
    pub fn file_type(&self) -> &str {
        match self.mime_type.as_str() {
            "image/png" | "image/jpeg" | "image/gif" | "image/webp" => "Image",
            "application/pdf" => "PDF",
            "text/plain" => "Text",
            "text/markdown" => "Markdown",
            "application/json" => "JSON",
            "video/mp4" | "video/webm" => "Video",
            "audio/mpeg" | "audio/ogg" => "Audio",
            _ => "File",
        }
    }

    /// Returns an icon character for this artifact type.
    pub fn icon(&self) -> &str {
        match self.mime_type.as_str() {
            "image/png" | "image/jpeg" | "image/gif" | "image/webp" => "🖼",
            "application/pdf" => "📄",
            "text/plain" | "text/markdown" => "📝",
            "application/json" => "{ }",
            "video/mp4" | "video/webm" => "🎬",
            "audio/mpeg" | "audio/ogg" => "🎵",
            _ => "📁",
        }
    }

    /// Returns a human-readable file size.
    pub fn size_display(&self) -> String {
        if self.size < 1024 {
            format!("{} B", self.size)
        } else if self.size < 1024 * 1024 {
            format!("{:.1} KB", self.size as f64 / 1024.0)
        } else {
            format!("{:.1} MB", self.size as f64 / (1024.0 * 1024.0))
        }
    }
}

/// State for tracking all artifacts.
#[derive(Debug, Clone, Default)]
pub struct ArtifactsState {
    /// Map of artifact_id -> Artifact
    pub artifacts: HashMap<String, Artifact>,
}

impl ArtifactsState {
    /// Creates a new empty artifacts state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Processes a home realm event that may affect artifacts.
    pub fn process_event(&mut self, event: &HomeRealmEvent) {
        match event {
            HomeRealmEvent::ArtifactUploaded {
                artifact_id,
                size,
                mime_type,
                tick,
                ..
            } => {
                let artifact =
                    Artifact::new(artifact_id.clone(), *size, mime_type.clone(), *tick);
                self.artifacts.insert(artifact_id.clone(), artifact);
            }
            HomeRealmEvent::ArtifactRetrieved { artifact_id, .. } => {
                if let Some(artifact) = self.artifacts.get_mut(artifact_id) {
                    artifact.retrieved_count += 1;
                }
            }
            _ => {}
        }
    }

    /// Returns the total count of artifacts.
    pub fn count(&self) -> usize {
        self.artifacts.len()
    }

    /// Returns the total size of all artifacts.
    pub fn total_size(&self) -> u64 {
        self.artifacts.values().map(|a| a.size).sum()
    }

    /// Returns a human-readable total size.
    pub fn total_size_display(&self) -> String {
        let size = self.total_size();
        if size < 1024 {
            format!("{} B", size)
        } else if size < 1024 * 1024 {
            format!("{:.1} KB", size as f64 / 1024.0)
        } else {
            format!("{:.1} MB", size as f64 / (1024.0 * 1024.0))
        }
    }

    /// Returns artifacts sorted by upload tick (newest first).
    pub fn artifacts_by_recency(&self) -> Vec<&Artifact> {
        let mut artifacts: Vec<_> = self.artifacts.values().collect();
        artifacts.sort_by(|a, b| b.uploaded_tick.cmp(&a.uploaded_tick));
        artifacts
    }

    /// Resets the state.
    pub fn reset(&mut self) {
        self.artifacts.clear();
    }
}
