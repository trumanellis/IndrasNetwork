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
    Transferred,
    Expired,
}

impl ArtifactStatus {
    pub fn css_class(&self) -> &'static str {
        match self {
            ArtifactStatus::Shared => "artifact-shared",
            ArtifactStatus::Recalled => "artifact-recalled",
            ArtifactStatus::Transferred => "artifact-transferred",
            ArtifactStatus::Expired => "artifact-expired",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            ArtifactStatus::Shared => "Shared",
            ArtifactStatus::Recalled => "Recalled",
            ArtifactStatus::Transferred => "Transferred",
            ArtifactStatus::Expired => "Expired",
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
    /// Pre-computed data URL for image thumbnails
    pub data_url: Option<String>,
    // New fields for shared filesystem
    /// Who owns this artifact
    pub owner: String,
    /// Access mode: "revocable", "permanent", "timed", "transfer"
    pub access_mode: String,
    /// Whether the current user can download this artifact
    pub downloadable: bool,
    /// When access expires (for timed grants), as tick timestamp
    pub expires_at: Option<u64>,
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

/// Breadcrumb entry for folder navigation
#[derive(Clone, Debug, PartialEq)]
pub struct FolderBreadcrumb {
    pub folder_id: String,
    pub title: String,
}

/// Navigation state for folder browsing
#[derive(Clone, Debug, Default)]
pub struct FolderNavigation {
    /// Breadcrumb path (empty = root)
    pub path: Vec<FolderBreadcrumb>,
    /// Current folder ID (None = root view)
    pub current_folder: Option<String>,
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
    /// Navigation state for folder browsing
    pub navigation: FolderNavigation,
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
                let data_url = asset_path.as_ref()
                    .and_then(|p| super::load_image_as_data_url(p));
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
                    data_url,
                    owner: sharer.clone(),
                    access_mode: "revocable".to_string(),
                    downloadable: false,
                    expires_at: None,
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

            StreamEvent::ProofFolderSubmitted {
                tick,
                realm_id,
                claimant,
                artifacts,
                ..
            } => {
                for art in artifacts {
                    let data_url = art.asset_path.as_ref()
                        .and_then(|p| super::load_image_as_data_url(p));
                    let info = ArtifactInfo {
                        artifact_hash: art.artifact_hash.clone(),
                        realm_id: realm_id.clone(),
                        name: art.name.clone(),
                        size: art.size,
                        mime_type: Some(art.mime_type.clone()),
                        sharer: claimant.clone(),
                        status: ArtifactStatus::Shared,
                        shared_at_tick: *tick,
                        recalled_at_tick: None,
                        recall_acknowledgments: Vec::new(),
                        asset_path: art.asset_path.clone(),
                        data_url,
                        owner: claimant.clone(),
                        access_mode: "permanent".to_string(),
                        downloadable: true,
                        expires_at: None,
                    };
                    self.artifacts.insert(art.artifact_hash.clone(), info);
                    self.total_shared += 1;
                }
            }

            StreamEvent::ArtifactUploaded {
                tick,
                owner,
                artifact_hash,
                name,
                size,
                mime_type,
                asset_path,
            } => {
                let data_url = asset_path.as_ref()
                    .and_then(|p| super::load_image_as_data_url(p));
                let info = ArtifactInfo {
                    artifact_hash: artifact_hash.clone(),
                    realm_id: String::new(),
                    name: name.clone(),
                    size: *size,
                    mime_type: mime_type.clone(),
                    sharer: owner.clone(),
                    status: ArtifactStatus::Shared,
                    shared_at_tick: *tick,
                    recalled_at_tick: None,
                    recall_acknowledgments: Vec::new(),
                    asset_path: asset_path.clone(),
                    data_url,
                    owner: owner.clone(),
                    access_mode: "permanent".to_string(),
                    downloadable: true,
                    expires_at: None,
                };
                self.artifacts.insert(artifact_hash.clone(), info);
                self.total_shared += 1;
            }

            StreamEvent::ArtifactGranted {
                artifact_hash,
                mode,
                ..
            } => {
                if let Some(artifact) = self.artifacts.get_mut(artifact_hash) {
                    artifact.access_mode = mode.clone();
                    artifact.downloadable = mode == "permanent";
                }
            }

            StreamEvent::ArtifactAccessRevoked {
                artifact_hash,
                ..
            } => {
                self.artifacts.remove(artifact_hash);
            }

            StreamEvent::ArtifactTransferred {
                artifact_hash,
                ..
            } => {
                if let Some(artifact) = self.artifacts.get_mut(artifact_hash) {
                    artifact.status = ArtifactStatus::Transferred;
                }
            }

            StreamEvent::ArtifactExpired {
                artifact_hash,
                ..
            } => {
                if let Some(artifact) = self.artifacts.get_mut(artifact_hash) {
                    artifact.status = ArtifactStatus::Expired;
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

    /// Get artifacts owned by a specific member (everything they shared with us)
    pub fn artifacts_by_owner(&self, owner: &str) -> Vec<&ArtifactInfo> {
        self.all_artifacts()
            .into_iter()
            .filter(|a| a.owner == owner)
            .collect()
    }

    /// Get downloadable artifacts only
    pub fn downloadable_artifacts(&self) -> Vec<&ArtifactInfo> {
        self.all_artifacts()
            .into_iter()
            .filter(|a| a.downloadable && a.status == ArtifactStatus::Shared)
            .collect()
    }

    /// Get transferred artifacts
    pub fn transferred_artifacts(&self) -> Vec<&ArtifactInfo> {
        self.all_artifacts()
            .into_iter()
            .filter(|a| a.status == ArtifactStatus::Transferred)
            .collect()
    }

    /// Get recent artifacts (up to limit), optionally filtered by status
    pub fn recent_artifacts(&self, limit: usize, status_filter: Option<ArtifactStatus>) -> Vec<&ArtifactInfo> {
        let artifacts = match status_filter {
            Some(ArtifactStatus::Shared) => self.shared_artifacts(),
            Some(ArtifactStatus::Recalled) => self.recalled_artifacts(),
            Some(ArtifactStatus::Transferred) => self.transferred_artifacts(),
            Some(ArtifactStatus::Expired) => self.all_artifacts()
                .into_iter()
                .filter(|a| a.status == ArtifactStatus::Expired)
                .collect(),
            None => self.all_artifacts(),
        };
        artifacts.into_iter().take(limit).collect()
    }

    /// Navigate into a folder
    pub fn open_folder(&mut self, folder_id: String, title: String) {
        self.navigation.path.push(FolderBreadcrumb {
            folder_id: folder_id.clone(),
            title,
        });
        self.navigation.current_folder = Some(folder_id);
    }

    /// Navigate to a breadcrumb level (0 = root)
    pub fn navigate_to(&mut self, index: usize) {
        if index == 0 {
            self.navigation.path.clear();
            self.navigation.current_folder = None;
        } else if index <= self.navigation.path.len() {
            self.navigation.path.truncate(index);
            self.navigation.current_folder = self.navigation.path.last().map(|b| b.folder_id.clone());
        }
    }

    /// Check if at root level
    pub fn is_at_root(&self) -> bool {
        self.navigation.current_folder.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::ProofArtifactItem;

    fn make_uploaded_event(tick: u32, owner: &str, hash: &str, name: &str, size: u64) -> StreamEvent {
        StreamEvent::ArtifactUploaded {
            tick,
            owner: owner.to_string(),
            artifact_hash: hash.to_string(),
            name: name.to_string(),
            size,
            mime_type: Some("application/octet-stream".to_string()),
            asset_path: None,
        }
    }

    fn make_shared_event(tick: u32, realm_id: &str, hash: &str, name: &str, size: u64, sharer: &str) -> StreamEvent {
        StreamEvent::ArtifactSharedRevocable {
            tick,
            realm_id: realm_id.to_string(),
            artifact_hash: hash.to_string(),
            name: name.to_string(),
            size,
            mime_type: Some("application/octet-stream".to_string()),
            sharer: sharer.to_string(),
            asset_path: None,
        }
    }

    #[test]
    fn test_artifact_uploaded_event() {
        let mut state = ArtifactState::default();
        let event = make_uploaded_event(1, "Zephyr", "hash1", "doc.pdf", 2048);
        state.process_event(&event);

        assert_eq!(state.artifacts.len(), 1);
        assert_eq!(state.total_shared, 1);
        let info = state.artifacts.get("hash1").unwrap();
        assert_eq!(info.owner, "Zephyr");
        assert_eq!(info.name, "doc.pdf");
        assert_eq!(info.size, 2048);
        assert_eq!(info.access_mode, "permanent");
        assert!(info.downloadable);
        assert_eq!(info.status, ArtifactStatus::Shared);
    }

    #[test]
    fn test_artifact_shared_revocable_event() {
        let mut state = ArtifactState::default();
        let event = make_shared_event(1, "realm1", "hash1", "image.png", 1024, "Nova");
        state.process_event(&event);

        assert_eq!(state.artifacts.len(), 1);
        let info = state.artifacts.get("hash1").unwrap();
        assert_eq!(info.sharer, "Nova");
        assert_eq!(info.owner, "Nova");
        assert_eq!(info.access_mode, "revocable");
        assert!(!info.downloadable);
        assert_eq!(info.realm_id, "realm1");
    }

    #[test]
    fn test_artifact_granted_event() {
        let mut state = ArtifactState::default();
        // First upload
        state.process_event(&make_uploaded_event(1, "Sage", "hash1", "doc.pdf", 2048));
        // Then grant
        let grant_event = StreamEvent::ArtifactGranted {
            tick: 2,
            artifact_hash: "hash1".to_string(),
            grantee: "Orion".to_string(),
            mode: "permanent".to_string(),
            granted_by: "Sage".to_string(),
        };
        state.process_event(&grant_event);

        let info = state.artifacts.get("hash1").unwrap();
        assert_eq!(info.access_mode, "permanent");
        assert!(info.downloadable);
    }

    #[test]
    fn test_artifact_granted_revocable_not_downloadable() {
        let mut state = ArtifactState::default();
        state.process_event(&make_uploaded_event(1, "Sage", "hash1", "doc.pdf", 2048));
        let grant_event = StreamEvent::ArtifactGranted {
            tick: 2,
            artifact_hash: "hash1".to_string(),
            grantee: "Orion".to_string(),
            mode: "revocable".to_string(),
            granted_by: "Sage".to_string(),
        };
        state.process_event(&grant_event);

        let info = state.artifacts.get("hash1").unwrap();
        assert_eq!(info.access_mode, "revocable");
        assert!(!info.downloadable);
    }

    #[test]
    fn test_artifact_access_revoked_removes() {
        let mut state = ArtifactState::default();
        state.process_event(&make_uploaded_event(1, "Sage", "hash1", "doc.pdf", 2048));
        assert_eq!(state.artifacts.len(), 1);

        let revoke_event = StreamEvent::ArtifactAccessRevoked {
            tick: 2,
            artifact_hash: "hash1".to_string(),
            grantee: "Orion".to_string(),
            revoked_by: "Sage".to_string(),
        };
        state.process_event(&revoke_event);

        assert_eq!(state.artifacts.len(), 0);
    }

    #[test]
    fn test_artifact_transferred_status() {
        let mut state = ArtifactState::default();
        state.process_event(&make_uploaded_event(1, "Sage", "hash1", "doc.pdf", 2048));

        let transfer_event = StreamEvent::ArtifactTransferred {
            tick: 2,
            artifact_hash: "hash1".to_string(),
            from: "Sage".to_string(),
            to: "Orion".to_string(),
        };
        state.process_event(&transfer_event);

        let info = state.artifacts.get("hash1").unwrap();
        assert_eq!(info.status, ArtifactStatus::Transferred);
    }

    #[test]
    fn test_artifact_expired_status() {
        let mut state = ArtifactState::default();
        state.process_event(&make_uploaded_event(1, "Sage", "hash1", "doc.pdf", 2048));

        let expire_event = StreamEvent::ArtifactExpired {
            tick: 100,
            artifact_hash: "hash1".to_string(),
            grantee: "Orion".to_string(),
        };
        state.process_event(&expire_event);

        let info = state.artifacts.get("hash1").unwrap();
        assert_eq!(info.status, ArtifactStatus::Expired);
    }

    #[test]
    fn test_artifact_recalled_event() {
        let mut state = ArtifactState::default();
        state.process_event(&make_shared_event(1, "realm1", "hash1", "img.png", 1024, "Nova"));

        let recall_event = StreamEvent::ArtifactRecalled {
            tick: 2,
            realm_id: "realm1".to_string(),
            artifact_hash: "hash1".to_string(),
            revoked_by: "Nova".to_string(),
        };
        state.process_event(&recall_event);

        let info = state.artifacts.get("hash1").unwrap();
        assert_eq!(info.status, ArtifactStatus::Recalled);
        assert_eq!(info.recalled_at_tick, Some(2));
        assert_eq!(state.total_recalled, 1);
    }

    #[test]
    fn test_recall_acknowledged_event() {
        let mut state = ArtifactState::default();
        state.process_event(&make_shared_event(1, "realm1", "hash1", "img.png", 1024, "Nova"));

        let ack_event = StreamEvent::RecallAcknowledged {
            tick: 3,
            realm_id: "realm1".to_string(),
            artifact_hash: "hash1".to_string(),
            acknowledged_by: "Kai".to_string(),
            blob_deleted: true,
            key_removed: true,
        };
        state.process_event(&ack_event);

        let info = state.artifacts.get("hash1").unwrap();
        assert_eq!(info.recall_acknowledgments, vec!["Kai".to_string()]);
    }

    #[test]
    fn test_recall_acknowledged_dedup() {
        let mut state = ArtifactState::default();
        state.process_event(&make_shared_event(1, "realm1", "hash1", "img.png", 1024, "Nova"));

        let ack = StreamEvent::RecallAcknowledged {
            tick: 3,
            realm_id: "realm1".to_string(),
            artifact_hash: "hash1".to_string(),
            acknowledged_by: "Kai".to_string(),
            blob_deleted: true,
            key_removed: true,
        };
        state.process_event(&ack);
        state.process_event(&ack);

        let info = state.artifacts.get("hash1").unwrap();
        assert_eq!(info.recall_acknowledgments.len(), 1);
    }

    // Query method tests

    #[test]
    fn test_artifacts_by_owner() {
        let mut state = ArtifactState::default();
        state.process_event(&make_uploaded_event(1, "Sage", "hash1", "a.pdf", 100));
        state.process_event(&make_uploaded_event(2, "Orion", "hash2", "b.pdf", 200));
        state.process_event(&make_uploaded_event(3, "Sage", "hash3", "c.pdf", 300));

        let sage_artifacts = state.artifacts_by_owner("Sage");
        assert_eq!(sage_artifacts.len(), 2);

        let orion_artifacts = state.artifacts_by_owner("Orion");
        assert_eq!(orion_artifacts.len(), 1);
    }

    #[test]
    fn test_downloadable_artifacts() {
        let mut state = ArtifactState::default();
        // Uploaded = permanent = downloadable
        state.process_event(&make_uploaded_event(1, "Sage", "hash1", "a.pdf", 100));
        // Shared revocable = not downloadable
        state.process_event(&make_shared_event(2, "r1", "hash2", "b.pdf", 200, "Nova"));

        let downloadable = state.downloadable_artifacts();
        assert_eq!(downloadable.len(), 1);
        assert_eq!(downloadable[0].artifact_hash, "hash1");
    }

    #[test]
    fn test_transferred_artifacts() {
        let mut state = ArtifactState::default();
        state.process_event(&make_uploaded_event(1, "Sage", "hash1", "a.pdf", 100));
        state.process_event(&make_uploaded_event(2, "Sage", "hash2", "b.pdf", 200));

        let transfer_event = StreamEvent::ArtifactTransferred {
            tick: 3,
            artifact_hash: "hash1".to_string(),
            from: "Sage".to_string(),
            to: "Orion".to_string(),
        };
        state.process_event(&transfer_event);

        let transferred = state.transferred_artifacts();
        assert_eq!(transferred.len(), 1);
        assert_eq!(transferred[0].artifact_hash, "hash1");
    }

    #[test]
    fn test_shared_artifacts_excludes_recalled() {
        let mut state = ArtifactState::default();
        state.process_event(&make_shared_event(1, "r1", "hash1", "a.png", 100, "Nova"));
        state.process_event(&make_shared_event(2, "r1", "hash2", "b.png", 200, "Nova"));

        let recall_event = StreamEvent::ArtifactRecalled {
            tick: 3,
            realm_id: "r1".to_string(),
            artifact_hash: "hash1".to_string(),
            revoked_by: "Nova".to_string(),
        };
        state.process_event(&recall_event);

        assert_eq!(state.shared_artifacts().len(), 1);
        assert_eq!(state.recalled_artifacts().len(), 1);
    }

    #[test]
    fn test_recent_artifacts_with_status_filter() {
        let mut state = ArtifactState::default();
        state.process_event(&make_uploaded_event(1, "Sage", "hash1", "a.pdf", 100));
        state.process_event(&make_uploaded_event(2, "Sage", "hash2", "b.pdf", 200));

        let transfer = StreamEvent::ArtifactTransferred {
            tick: 3,
            artifact_hash: "hash1".to_string(),
            from: "Sage".to_string(),
            to: "Orion".to_string(),
        };
        state.process_event(&transfer);

        let all = state.recent_artifacts(10, None);
        assert_eq!(all.len(), 2);

        let shared = state.recent_artifacts(10, Some(ArtifactStatus::Shared));
        assert_eq!(shared.len(), 1);

        let transferred = state.recent_artifacts(10, Some(ArtifactStatus::Transferred));
        assert_eq!(transferred.len(), 1);
    }

    // Folder navigation tests

    #[test]
    fn test_folder_navigation() {
        let mut state = ArtifactState::default();
        assert!(state.is_at_root());

        state.open_folder("folder1".to_string(), "Photos".to_string());
        assert!(!state.is_at_root());
        assert_eq!(state.navigation.current_folder, Some("folder1".to_string()));
        assert_eq!(state.navigation.path.len(), 1);

        state.open_folder("folder2".to_string(), "Vacation".to_string());
        assert_eq!(state.navigation.path.len(), 2);
        assert_eq!(state.navigation.current_folder, Some("folder2".to_string()));

        // Navigate back to first level
        state.navigate_to(1);
        assert_eq!(state.navigation.path.len(), 1);
        assert_eq!(state.navigation.current_folder, Some("folder1".to_string()));

        // Navigate to root
        state.navigate_to(0);
        assert!(state.is_at_root());
        assert_eq!(state.navigation.path.len(), 0);
    }

    #[test]
    fn test_proof_folder_submitted_event() {
        let mut state = ArtifactState::default();

        let event = StreamEvent::ProofFolderSubmitted {
            tick: 5,
            realm_id: "realm1".to_string(),
            claimant: "Lyra".to_string(),
            quest_id: "q1".to_string(),
            quest_title: "Test Quest".to_string(),
            folder_id: "f1".to_string(),
            narrative: "Done".to_string(),
            narrative_preview: "Done".to_string(),
            artifact_count: 1,
            artifacts: vec![
                ProofArtifactItem {
                    artifact_hash: "proof_hash".to_string(),
                    name: "proof.png".to_string(),
                    size: 500,
                    mime_type: "image/png".to_string(),
                    thumbnail_data: None,
                    inline_data: None,
                    dimensions: None,
                    asset_path: None,
                    caption: None,
                },
            ],
        };
        state.process_event(&event);

        assert_eq!(state.artifacts.len(), 1);
        let info = state.artifacts.get("proof_hash").unwrap();
        assert_eq!(info.sharer, "Lyra");
        assert_eq!(info.access_mode, "permanent");
        assert!(info.downloadable);
    }
}
