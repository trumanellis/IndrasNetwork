//! Artifact tracking state
//!
//! Tracks shared artifacts and their revocation status.

use std::collections::HashMap;

use crate::events::StreamEvent;

/// Status of a shared artifact
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum ArtifactStatus {
    #[default]
    Shared,
    Recalled,
}

impl ArtifactStatus {
    pub fn css_class(&self) -> &'static str {
        match self {
            ArtifactStatus::Shared => "artifact-shared",
            ArtifactStatus::Recalled => "artifact-recalled",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            ArtifactStatus::Shared => "Shared",
            ArtifactStatus::Recalled => "Recalled",
        }
    }
}

/// Information about a shared artifact
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ArtifactInfo {
    pub artifact_hash: String,
    pub realm_id: String,
    pub name: String,
    pub size: u64,
    pub mime_type: Option<String>,
    pub sharer: String,
    pub status: ArtifactStatus,
    pub shared_at_tick: u32,
    pub recalled_at_tick: Option<u32>,
    /// Members who have acknowledged recall
    pub recall_acknowledgments: Vec<String>,
    /// Local file path for real assets (if available)
    pub asset_path: Option<String>,
}

impl ArtifactInfo {
    /// Check if this artifact has a local image that can be displayed
    pub fn has_displayable_image(&self) -> bool {
        if self.asset_path.is_none() {
            return false;
        }

        // Check mime type
        if let Some(ref mime) = self.mime_type {
            if mime.starts_with("image/") {
                return true;
            }
        }

        // Check file extension
        if let Some(ext) = self.name.rsplit('.').next() {
            matches!(ext.to_lowercase().as_str(), "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg")
        } else {
            false
        }
    }

    /// Get a file extension icon based on mime type or name
    pub fn icon(&self) -> &'static str {
        if let Some(ref mime) = self.mime_type {
            if mime.starts_with("image/") {
                return "🖼️";
            } else if mime.starts_with("video/") {
                return "🎬";
            } else if mime.starts_with("audio/") {
                return "🎵";
            } else if mime.starts_with("text/") {
                return "📄";
            } else if mime == "application/pdf" {
                return "📕";
            }
        }

        // Fallback to extension-based
        if let Some(ext) = self.name.rsplit('.').next() {
            match ext.to_lowercase().as_str() {
                "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" => "🖼️",
                "mp4" | "mov" | "avi" | "mkv" | "webm" => "🎬",
                "mp3" | "wav" | "flac" | "ogg" | "m4a" => "🎵",
                "pdf" => "📕",
                "doc" | "docx" => "📘",
                "xls" | "xlsx" => "📗",
                "zip" | "tar" | "gz" | "rar" => "📦",
                "rs" | "py" | "js" | "ts" | "lua" => "💻",
                _ => "📄",
            }
        } else {
            "📄"
        }
    }

    /// Format file size for display
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
}

/// State for tracking artifacts
#[derive(Clone, Debug, Default)]
pub struct ArtifactState {
    /// All artifacts by hash
    pub artifacts: HashMap<String, ArtifactInfo>,
    /// Total artifacts shared
    pub total_shared: usize,
    /// Total artifacts recalled
    pub total_recalled: usize,
}

impl ArtifactState {
    /// Process an artifact-related event
    pub fn process_event(&mut self, event: &StreamEvent) {
        match event {
            StreamEvent::ArtifactSharedRevocable {
                tick,
                realm_id,
                artifact_hash,
                name,
                size,
                mime_type,
                sharer,
                asset_path,
            } => {
                let info = ArtifactInfo {
                    artifact_hash: artifact_hash.clone(),
                    realm_id: realm_id.clone(),
                    name: name.clone(),
                    size: *size,
                    mime_type: mime_type.clone(),
                    sharer: sharer.clone(),
                    status: ArtifactStatus::Shared,
                    shared_at_tick: *tick,
                    recalled_at_tick: None,
                    recall_acknowledgments: Vec::new(),
                    asset_path: asset_path.clone(),
                };
                self.artifacts.insert(artifact_hash.clone(), info);
                self.total_shared += 1;
            }

            StreamEvent::ArtifactRecalled {
                tick,
                artifact_hash,
                ..
            } => {
                if let Some(artifact) = self.artifacts.get_mut(artifact_hash) {
                    artifact.status = ArtifactStatus::Recalled;
                    artifact.recalled_at_tick = Some(*tick);
                    self.total_recalled += 1;
                }
            }

            StreamEvent::RecallAcknowledged {
                artifact_hash,
                acknowledged_by,
                ..
            } => {
                if let Some(artifact) = self.artifacts.get_mut(artifact_hash) {
                    if !artifact.recall_acknowledgments.contains(acknowledged_by) {
                        artifact.recall_acknowledgments.push(acknowledged_by.clone());
                    }
                }
            }

            _ => {}
        }
    }

    /// Get all artifacts, newest first
    pub fn all_artifacts(&self) -> Vec<&ArtifactInfo> {
        let mut artifacts: Vec<_> = self.artifacts.values().collect();
        artifacts.sort_by(|a, b| b.shared_at_tick.cmp(&a.shared_at_tick));
        artifacts
    }

    /// Get artifacts that are still shared (not recalled)
    pub fn shared_artifacts(&self) -> Vec<&ArtifactInfo> {
        self.all_artifacts()
            .into_iter()
            .filter(|a| a.status == ArtifactStatus::Shared)
            .collect()
    }

    /// Get artifacts that have been recalled
    pub fn recalled_artifacts(&self) -> Vec<&ArtifactInfo> {
        self.all_artifacts()
            .into_iter()
            .filter(|a| a.status == ArtifactStatus::Recalled)
            .collect()
    }

    /// Get artifacts for a specific realm
    pub fn artifacts_for_realm(&self, realm_id: &str) -> Vec<&ArtifactInfo> {
        self.all_artifacts()
            .into_iter()
            .filter(|a| a.realm_id == realm_id)
            .collect()
    }

    /// Get artifacts shared by a specific member
    pub fn artifacts_by_member(&self, member: &str) -> Vec<&ArtifactInfo> {
        self.all_artifacts()
            .into_iter()
            .filter(|a| a.sharer == member)
            .collect()
    }

    /// Get recent artifacts (up to limit), optionally filtered by status
    pub fn recent_artifacts(&self, limit: usize, status_filter: Option<ArtifactStatus>) -> Vec<&ArtifactInfo> {
        let artifacts = match status_filter {
            Some(ArtifactStatus::Shared) => self.shared_artifacts(),
            Some(ArtifactStatus::Recalled) => self.recalled_artifacts(),
            None => self.all_artifacts(),
        };
        artifacts.into_iter().take(limit).collect()
    }
}
