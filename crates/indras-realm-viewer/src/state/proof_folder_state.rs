//! Proof Folder Editor State
//!
//! Manages UI state for creating and submitting proof folders.

/// Status of an artifact upload
#[derive(Clone, Debug, PartialEq)]
pub enum UploadStatus {
    Pending,
    Uploading { progress: f32 },
    Complete,
    Failed { error: String },
}

impl Default for UploadStatus {
    fn default() -> Self {
        Self::Pending
    }
}

/// A draft artifact being added to a proof folder
#[derive(Clone, Debug, PartialEq)]
pub struct DraftArtifact {
    pub id: String,
    pub name: String,
    pub size: u64,
    pub mime_type: Option<String>,
    pub caption: Option<String>,
    /// Local blob URL for image previews
    pub thumbnail_url: Option<String>,
    pub upload_status: UploadStatus,
}

impl DraftArtifact {
    pub fn new(id: String, name: String, size: u64, mime_type: Option<String>) -> Self {
        Self {
            id,
            name,
            size,
            mime_type,
            caption: None,
            thumbnail_url: None,
            upload_status: UploadStatus::Pending,
        }
    }

    /// Check if this artifact is an image based on mime type
    pub fn is_image(&self) -> bool {
        self.mime_type
            .as_ref()
            .map(|m| m.starts_with("image/"))
            .unwrap_or(false)
    }

    /// Get file extension icon (emoji-based)
    pub fn file_icon(&self) -> &'static str {
        let ext = self.name.rsplit('.').next().unwrap_or("");
        match ext.to_lowercase().as_str() {
            "pdf" => "📕",
            "doc" | "docx" => "📄",
            "xls" | "xlsx" => "📊",
            "ppt" | "pptx" => "📽",
            "mp4" | "mov" | "avi" | "webm" => "🎬",
            "mp3" | "wav" | "ogg" => "🎵",
            "zip" | "tar" | "gz" | "rar" => "📦",
            "txt" | "md" => "📝",
            "json" | "xml" | "yaml" => "⚙",
            _ => "📎",
        }
    }

    /// Format file size for display
    pub fn formatted_size(&self) -> String {
        if self.size < 1024 {
            format!("{} B", self.size)
        } else if self.size < 1024 * 1024 {
            format!("{:.1} KB", self.size as f64 / 1024.0)
        } else {
            format!("{:.1} MB", self.size as f64 / (1024.0 * 1024.0))
        }
    }
}

/// A draft proof folder being edited
#[derive(Clone, Debug, Default)]
pub struct DraftProofFolder {
    /// None until created via API
    pub folder_id: Option<String>,
    pub quest_id: String,
    pub quest_title: String,
    pub narrative: String,
    pub artifacts: Vec<DraftArtifact>,
    pub is_dirty: bool,
}

impl DraftProofFolder {
    pub fn new(quest_id: String, quest_title: String) -> Self {
        Self {
            folder_id: None,
            quest_id,
            quest_title,
            narrative: String::new(),
            artifacts: Vec::new(),
            is_dirty: false,
        }
    }

    pub fn add_artifact(&mut self, artifact: DraftArtifact) {
        self.artifacts.push(artifact);
        self.is_dirty = true;
    }

    pub fn remove_artifact(&mut self, artifact_id: &str) {
        self.artifacts.retain(|a| a.id != artifact_id);
        self.is_dirty = true;
    }

    pub fn set_narrative(&mut self, narrative: String) {
        self.narrative = narrative;
        self.is_dirty = true;
    }

    pub fn set_artifact_caption(&mut self, artifact_id: &str, caption: String) {
        if let Some(artifact) = self.artifacts.iter_mut().find(|a| a.id == artifact_id) {
            artifact.caption = Some(caption);
            self.is_dirty = true;
        }
    }

    /// Get a preview of the narrative (first 50 chars)
    pub fn narrative_preview(&self) -> String {
        if self.narrative.len() <= 50 {
            self.narrative.clone()
        } else {
            format!("{}...", &self.narrative[..47])
        }
    }
}

/// Editor mode for proof folder UI
#[derive(Clone, Debug, PartialEq)]
pub enum EditorMode {
    /// Editor is hidden, chat is showing
    Hidden,
    /// Creating a new proof folder for a quest
    Creating { quest_id: String },
    /// Editing an existing proof folder
    Editing { folder_id: String },
}

impl Default for EditorMode {
    fn default() -> Self {
        Self::Hidden
    }
}

/// State for the proof folder editor
#[derive(Clone, Debug, Default)]
pub struct ProofFolderState {
    pub editor_mode: EditorMode,
    pub current_draft: Option<DraftProofFolder>,
    /// Whether the quest selector is showing (for chat entry point)
    pub showing_quest_selector: bool,
}

impl ProofFolderState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Open the editor for a new proof folder
    pub fn open_for_quest(&mut self, quest_id: String, quest_title: String) {
        self.editor_mode = EditorMode::Creating { quest_id: quest_id.clone() };
        self.current_draft = Some(DraftProofFolder::new(quest_id, quest_title));
        self.showing_quest_selector = false;
    }

    /// Open the editor for an existing proof folder
    pub fn open_for_editing(&mut self, folder_id: String, draft: DraftProofFolder) {
        self.editor_mode = EditorMode::Editing { folder_id };
        self.current_draft = Some(draft);
        self.showing_quest_selector = false;
    }

    /// Close the editor and return to chat
    pub fn close(&mut self) {
        self.editor_mode = EditorMode::Hidden;
        self.current_draft = None;
        self.showing_quest_selector = false;
    }

    /// Check if editor is currently open
    pub fn is_open(&self) -> bool {
        !matches!(self.editor_mode, EditorMode::Hidden)
    }

    /// Show the quest selector (for chat "+" menu entry point)
    pub fn show_quest_selector(&mut self) {
        self.showing_quest_selector = true;
    }

    /// Hide the quest selector
    pub fn hide_quest_selector(&mut self) {
        self.showing_quest_selector = false;
    }

    /// Get the current draft mutably
    pub fn draft_mut(&mut self) -> Option<&mut DraftProofFolder> {
        self.current_draft.as_mut()
    }

    /// Check if current draft has unsaved changes
    pub fn has_unsaved_changes(&self) -> bool {
        self.current_draft
            .as_ref()
            .map(|d| d.is_dirty)
            .unwrap_or(false)
    }
}

// ============================================================================
// PER-MEMBER PROOF DRAFT STATE (for Omni V2 multi-column dashboard)
// ============================================================================

use std::collections::HashMap;
use crate::events::StreamEvent;

/// Info about an artifact in a proof folder draft
#[derive(Clone, Debug, PartialEq)]
pub struct DraftArtifactInfo {
    pub artifact_id: String,
    pub name: String,
    pub size: u64,
    pub mime_type: String,
    pub added_at_tick: u32,
    /// Data URL for image/video preview (built from asset_path)
    pub data_url: Option<String>,
    /// Caption / alt text
    pub caption: Option<String>,
}

impl DraftArtifactInfo {
    /// Check if this is an image
    pub fn is_image(&self) -> bool {
        self.mime_type.starts_with("image/")
    }

    /// Check if this is a video
    pub fn is_video(&self) -> bool {
        self.mime_type.starts_with("video/")
    }
}

/// A per-member proof folder draft (tracked from stream events)
#[derive(Clone, Debug, PartialEq)]
pub struct MemberProofDraft {
    pub folder_id: String,
    pub quest_id: String,
    pub realm_id: String,
    pub narrative_length: usize,
    /// Full markdown narrative text (for rendered preview)
    pub narrative: String,
    pub artifacts: Vec<DraftArtifactInfo>,
    pub created_at_tick: u32,
    pub last_updated_tick: u32,
}

/// Tracks per-member proof folder drafts from stream events.
/// Unlike `ProofFolderState` (which is a local UI editor state),
/// this tracks what each member is doing based on incoming events.
#[derive(Clone, Debug, Default)]
pub struct MemberProofDraftState {
    pub drafts: HashMap<String, MemberProofDraft>,
    pub folder_to_member: HashMap<String, String>,
}

impl MemberProofDraftState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Process a stream event and update draft state
    pub fn process_event(&mut self, event: &StreamEvent) {
        match event {
            StreamEvent::ProofFolderCreated {
                tick,
                realm_id,
                quest_id,
                folder_id,
                claimant,
                ..
            } => {
                self.drafts.insert(
                    claimant.clone(),
                    MemberProofDraft {
                        folder_id: folder_id.clone(),
                        quest_id: quest_id.clone(),
                        realm_id: realm_id.clone(),
                        narrative_length: 0,
                        narrative: String::new(),
                        artifacts: Vec::new(),
                        created_at_tick: *tick,
                        last_updated_tick: *tick,
                    },
                );
                self.folder_to_member
                    .insert(folder_id.clone(), claimant.clone());
            }

            StreamEvent::ProofFolderNarrativeUpdated {
                tick,
                folder_id,
                narrative_length,
                narrative,
                ..
            } => {
                if let Some(member_id) = self.folder_to_member.get(folder_id).cloned() {
                    if let Some(draft) = self.drafts.get_mut(&member_id) {
                        draft.narrative_length = *narrative_length;
                        if !narrative.is_empty() {
                            draft.narrative = narrative.clone();
                        }
                        draft.last_updated_tick = *tick;
                    }
                }
            }

            StreamEvent::ProofFolderArtifactAdded {
                tick,
                folder_id,
                artifact_id,
                artifact_name,
                artifact_size,
                mime_type,
                asset_path,
                caption,
                ..
            } => {
                if let Some(member_id) = self.folder_to_member.get(folder_id).cloned() {
                    if let Some(draft) = self.drafts.get_mut(&member_id) {
                        // Build data URL from asset_path if available
                        let data_url = asset_path.as_ref()
                            .and_then(|path| super::load_image_as_data_url(path));
                        draft.artifacts.push(DraftArtifactInfo {
                            artifact_id: artifact_id.clone(),
                            name: artifact_name.clone(),
                            size: *artifact_size,
                            mime_type: mime_type.clone(),
                            added_at_tick: *tick,
                            data_url,
                            caption: caption.clone(),
                        });
                        draft.last_updated_tick = *tick;
                    }
                }
            }

            StreamEvent::ProofFolderSubmitted {
                folder_id,
                claimant,
                ..
            } => {
                self.drafts.remove(claimant);
                self.folder_to_member.remove(folder_id);
            }

            _ => {}
        }
    }

    /// Get the active draft for a member, if any
    pub fn draft_for_member(&self, member: &str) -> Option<&MemberProofDraft> {
        self.drafts.get(member)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_draft_artifact_file_icon() {
        let pdf = DraftArtifact::new("1".into(), "doc.pdf".into(), 1000, None);
        assert_eq!(pdf.file_icon(), "📕");

        let video = DraftArtifact::new("2".into(), "video.mp4".into(), 1000, None);
        assert_eq!(video.file_icon(), "🎬");

        let unknown = DraftArtifact::new("3".into(), "file.xyz".into(), 1000, None);
        assert_eq!(unknown.file_icon(), "📎");
    }

    #[test]
    fn test_draft_artifact_formatted_size() {
        let small = DraftArtifact::new("1".into(), "small.txt".into(), 500, None);
        assert_eq!(small.formatted_size(), "500 B");

        let medium = DraftArtifact::new("2".into(), "medium.txt".into(), 2048, None);
        assert_eq!(medium.formatted_size(), "2.0 KB");

        let large = DraftArtifact::new("3".into(), "large.txt".into(), 2 * 1024 * 1024, None);
        assert_eq!(large.formatted_size(), "2.0 MB");
    }

    #[test]
    fn test_proof_folder_state_open_close() {
        let mut state = ProofFolderState::new();
        assert!(!state.is_open());

        state.open_for_quest("quest1".into(), "Test Quest".into());
        assert!(state.is_open());
        assert!(state.current_draft.is_some());

        state.close();
        assert!(!state.is_open());
        assert!(state.current_draft.is_none());
    }

    #[test]
    fn test_member_proof_draft_lifecycle() {
        let mut state = MemberProofDraftState::new();

        // Create a proof folder for a member
        state.process_event(&StreamEvent::ProofFolderCreated {
            tick: 10,
            realm_id: "realm1".into(),
            quest_id: "quest1".into(),
            folder_id: "folder1".into(),
            claimant: "member1".into(),
            status: "draft".into(),
        });

        let draft = state.draft_for_member("member1").unwrap();
        assert_eq!(draft.folder_id, "folder1");
        assert_eq!(draft.quest_id, "quest1");
        assert_eq!(draft.narrative_length, 0);
        assert!(draft.artifacts.is_empty());

        // Update narrative
        state.process_event(&StreamEvent::ProofFolderNarrativeUpdated {
            tick: 15,
            realm_id: "realm1".into(),
            folder_id: "folder1".into(),
            claimant: "member1".into(),
            narrative_length: 250,
            narrative: "## Work Completed\n\nDid the thing.".into(),
        });

        let draft = state.draft_for_member("member1").unwrap();
        assert_eq!(draft.narrative_length, 250);
        assert_eq!(draft.narrative, "## Work Completed\n\nDid the thing.");
        assert_eq!(draft.last_updated_tick, 15);

        // Add artifact
        state.process_event(&StreamEvent::ProofFolderArtifactAdded {
            tick: 20,
            realm_id: "realm1".into(),
            folder_id: "folder1".into(),
            artifact_id: "art1".into(),
            artifact_name: "photo.jpg".into(),
            artifact_size: 1024,
            mime_type: "image/jpeg".into(),
            asset_path: None,
            caption: Some("Evidence photo".into()),
        });

        let draft = state.draft_for_member("member1").unwrap();
        assert_eq!(draft.artifacts.len(), 1);
        assert_eq!(draft.artifacts[0].name, "photo.jpg");

        // Submit clears the draft
        state.process_event(&StreamEvent::ProofFolderSubmitted {
            tick: 25,
            realm_id: "realm1".into(),
            quest_id: "quest1".into(),
            claimant: "member1".into(),
            folder_id: "folder1".into(),
            artifact_count: 1,
            narrative_preview: "My story".into(),
            quest_title: "Test Quest".into(),
            narrative: String::new(),
            artifacts: Vec::new(),
        });

        assert!(state.draft_for_member("member1").is_none());
        assert!(state.folder_to_member.get("folder1").is_none());
    }
}
