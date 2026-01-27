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
}
