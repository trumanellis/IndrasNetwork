//! Chat and Blessing State
//!
//! Tracks realm chat messages, proof submissions, and blessings.
//! Supports editable messages with version history.

use crate::events::StreamEvent;
use std::collections::{HashMap, VecDeque};

/// Maximum chat messages to keep per realm.
const MAX_CHAT_MESSAGES: usize = 100;

/// A previous version of a chat message.
#[derive(Clone, Debug, PartialEq)]
pub struct MessageVersion {
    /// The content at this version.
    pub content: String,
    /// Tick when this edit was made.
    pub edited_at: u32,
}

/// A chat message in the realm.
#[derive(Clone, Debug, PartialEq)]
pub struct ChatMessage {
    /// Unique message ID.
    pub id: String,
    pub tick: u32,
    pub member: String,
    pub content: String,
    pub message_type: ChatMessageType,
    /// Edit history (oldest to newest).
    pub versions: Vec<MessageVersion>,
    /// Whether this message has been deleted.
    pub is_deleted: bool,
}

impl ChatMessage {
    /// Create a new chat message.
    pub fn new(id: String, tick: u32, member: String, content: String, message_type: ChatMessageType) -> Self {
        Self {
            id,
            tick,
            member,
            content,
            message_type,
            versions: Vec::new(),
            is_deleted: false,
        }
    }

    /// Check if this message has been edited.
    pub fn is_edited(&self) -> bool {
        !self.versions.is_empty()
    }

    /// Get total version count (current + history).
    pub fn version_count(&self) -> usize {
        self.versions.len() + 1
    }

    /// Check if the given member can edit this message.
    pub fn can_edit(&self, member_id: &str) -> bool {
        self.member == member_id && !self.is_deleted
    }
}

/// Types of chat messages.
#[derive(Clone, Debug, PartialEq)]
pub enum ChatMessageType {
    /// Regular text message.
    Text,
    /// Proof submission for a quest.
    ProofSubmitted {
        quest_id: String,
        quest_title: String,
        artifact_id: String,
        artifact_name: String,
    },
    /// Proof folder submitted for a quest.
    ProofFolderSubmitted {
        quest_id: String,
        folder_id: String,
        artifact_count: usize,
        narrative_preview: String,
        /// Quest title for display.
        quest_title: String,
        /// Full markdown narrative.
        narrative: String,
        /// Artifacts in the proof folder.
        artifacts: Vec<ProofArtifactStateItem>,
    },
    /// Blessing given to a proof.
    BlessingGiven {
        quest_id: String,
        claimant: String,
        attention_millis: u64,
    },
    /// Image shared inline in chat.
    Image {
        /// MIME type (image/png, image/jpeg, etc.)
        mime_type: String,
        /// Base64-encoded image data (for embedded images).
        inline_data: Option<String>,
        /// Artifact hash for large images (hex string).
        artifact_hash: Option<String>,
        /// Original filename.
        filename: Option<String>,
        /// Image dimensions (width, height).
        dimensions: Option<(u32, u32)>,
        /// Alt text / caption.
        alt_text: Option<String>,
        /// Local asset path for viewer testing.
        asset_path: Option<String>,
    },
    /// Gallery of images/videos/files.
    Gallery {
        /// Unique folder identifier.
        folder_id: String,
        /// Gallery title.
        title: Option<String>,
        /// Items in the gallery.
        items: Vec<GalleryStateItem>,
    },
}

/// Item in a gallery.
#[derive(Clone, Debug, PartialEq)]
pub struct GalleryStateItem {
    /// Filename.
    pub name: String,
    /// MIME type.
    pub mime_type: String,
    /// Size in bytes.
    pub size: u64,
    /// Base64-encoded thumbnail (for images/videos).
    pub thumbnail_data: Option<String>,
    /// Text preview (first ~200 chars for text/markdown files).
    pub text_preview: Option<String>,
    /// Artifact hash reference (hex string).
    pub artifact_hash: String,
    /// Item dimensions (width, height) for images.
    pub dimensions: Option<(u32, u32)>,
    /// Local asset path for viewer testing.
    pub asset_path: Option<String>,
}

impl GalleryStateItem {
    /// Check if this is an image item.
    pub fn is_image(&self) -> bool {
        self.mime_type.starts_with("image/")
    }

    /// Check if this is a text/markdown item.
    pub fn is_text(&self) -> bool {
        self.mime_type.starts_with("text/") || self.mime_type == "application/markdown"
    }

    /// Get the file extension icon.
    pub fn icon(&self) -> &'static str {
        if self.is_image() {
            "🖼️"
        } else if self.mime_type.contains("markdown") || self.name.ends_with(".md") {
            "📝"
        } else if self.mime_type.starts_with("text/") {
            "📄"
        } else if self.mime_type.contains("pdf") {
            "📕"
        } else if self.mime_type.contains("zip") || self.mime_type.contains("archive") {
            "📦"
        } else {
            "📎"
        }
    }
}

/// Artifact item in a proof folder (state representation).
#[derive(Clone, Debug, PartialEq)]
pub struct ProofArtifactStateItem {
    /// BLAKE3 hash of artifact (hex).
    pub artifact_hash: String,
    /// Filename.
    pub name: String,
    /// MIME type.
    pub mime_type: String,
    /// Size in bytes.
    pub size: u64,
    /// Data URL for display (built from inline_data or asset_path).
    pub data_url: Option<String>,
    /// Image dimensions (width, height).
    pub dimensions: Option<(u32, u32)>,
    /// Caption / alt text.
    pub caption: Option<String>,
}

impl ProofArtifactStateItem {
    /// Check if this is an image item.
    pub fn is_image(&self) -> bool {
        self.mime_type.starts_with("image/")
    }
}

/// Blessing information for a proof.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ProofBlessingInfo {
    pub quest_id: String,
    pub quest_title: String,
    pub claimant: String,
    pub artifact_id: String,
    pub artifact_name: String,
    pub tick: u32,
    /// Total blessed attention in milliseconds.
    pub total_blessed_millis: u64,
    /// Number of members who blessed.
    pub blesser_count: usize,
    /// Individual blessings.
    pub blessings: Vec<BlessingInfo>,
}

/// Individual blessing.
#[derive(Clone, Debug, PartialEq)]
pub struct BlessingInfo {
    pub blesser: String,
    pub attention_millis: u64,
    pub tick: u32,
}

/// State tracking for chat messages and blessings.
#[derive(Clone, Debug, Default)]
pub struct ChatState {
    /// Chat messages by realm (newest first).
    pub messages_by_realm: HashMap<String, VecDeque<ChatMessage>>,
    /// Global chat feed (newest first).
    pub global_messages: VecDeque<ChatMessage>,
    /// Proof blessings indexed by (quest_id, claimant).
    pub proof_blessings: HashMap<(String, String), ProofBlessingInfo>,
    /// Total message count.
    pub total_messages: usize,
    /// Total blessings.
    pub total_blessings: usize,
}

impl ChatState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Process a stream event.
    pub fn process_event(&mut self, event: &StreamEvent) {
        match event {
            StreamEvent::ChatMessage {
                tick,
                member,
                content,
                message_id,
                realm_id,
                ..
            } => {
                // Generate ID if not provided
                let id = message_id.clone()
                    .unwrap_or_else(|| format!("msg-{}-{}", tick, member));
                let msg = ChatMessage::new(
                    id,
                    *tick,
                    member.clone(),
                    content.clone(),
                    ChatMessageType::Text,
                );
                if let Some(rid) = realm_id {
                    self.add_realm_message(rid, msg.clone());
                }
                self.add_global_message(msg);
            }

            StreamEvent::ChatMessageEdited {
                tick,
                message_id,
                new_content,
                ..
            } => {
                self.edit_message(message_id, new_content.clone(), *tick);
            }

            StreamEvent::ChatMessageDeleted {
                tick,
                message_id,
                ..
            } => {
                self.delete_message(message_id, *tick);
            }

            StreamEvent::ProofSubmitted {
                tick,
                realm_id,
                quest_id,
                claimant,
                quest_title,
                artifact_id,
                artifact_name,
            } => {
                // Create proof blessing entry
                let key = (quest_id.clone(), claimant.clone());
                self.proof_blessings.insert(
                    key,
                    ProofBlessingInfo {
                        quest_id: quest_id.clone(),
                        quest_title: quest_title.clone(),
                        claimant: claimant.clone(),
                        artifact_id: artifact_id.clone(),
                        artifact_name: artifact_name.clone(),
                        tick: *tick,
                        total_blessed_millis: 0,
                        blesser_count: 0,
                        blessings: Vec::new(),
                    },
                );

                // Add chat message
                let id = format!("proof-{}-{}", quest_id, tick);
                let msg = ChatMessage::new(
                    id,
                    *tick,
                    claimant.clone(),
                    format!("Submitted proof for {}", quest_title),
                    ChatMessageType::ProofSubmitted {
                        quest_id: quest_id.clone(),
                        quest_title: quest_title.clone(),
                        artifact_id: artifact_id.clone(),
                        artifact_name: artifact_name.clone(),
                    },
                );
                self.add_realm_message(realm_id, msg.clone());
                self.add_global_message(msg);
            }

            StreamEvent::BlessingGiven {
                tick,
                realm_id,
                quest_id,
                claimant,
                blesser,
                attention_millis,
                ..
            } => {
                // Update proof blessing info
                let key = (quest_id.clone(), claimant.clone());
                if let Some(info) = self.proof_blessings.get_mut(&key) {
                    info.total_blessed_millis += attention_millis;
                    info.blesser_count += 1;
                    info.blessings.push(BlessingInfo {
                        blesser: blesser.clone(),
                        attention_millis: *attention_millis,
                        tick: *tick,
                    });
                }

                self.total_blessings += 1;

                // Add chat message
                let id = format!("blessing-{}-{}-{}", quest_id, blesser, tick);
                let msg = ChatMessage::new(
                    id,
                    *tick,
                    blesser.clone(),
                    format!(
                        "Blessed {} with {}",
                        claimant,
                        format_duration(*attention_millis)
                    ),
                    ChatMessageType::BlessingGiven {
                        quest_id: quest_id.clone(),
                        claimant: claimant.clone(),
                        attention_millis: *attention_millis,
                    },
                );
                self.add_realm_message(realm_id, msg.clone());
                self.add_global_message(msg);
            }

            StreamEvent::ProofFolderSubmitted {
                tick,
                realm_id,
                quest_id,
                claimant,
                folder_id,
                artifact_count,
                narrative_preview,
                quest_title,
                narrative,
                artifacts,
            } => {
                // Use quest_title if provided, otherwise fall back to empty
                let title = if quest_title.is_empty() {
                    String::new()
                } else {
                    quest_title.clone()
                };

                // Create proof blessing entry (similar to ProofSubmitted)
                let key = (quest_id.clone(), claimant.clone());
                self.proof_blessings.insert(
                    key,
                    ProofBlessingInfo {
                        quest_id: quest_id.clone(),
                        quest_title: title.clone(),
                        claimant: claimant.clone(),
                        artifact_id: folder_id.clone(),
                        artifact_name: format!("Proof folder ({} files)", artifact_count),
                        tick: *tick,
                        total_blessed_millis: 0,
                        blesser_count: 0,
                        blessings: Vec::new(),
                    },
                );

                // Convert artifacts to state items with data URLs
                let state_artifacts: Vec<ProofArtifactStateItem> = artifacts.iter().map(|item| {
                    // Build data URL: prefer asset_path, then inline_data
                    let data_url = item.asset_path.as_ref()
                        .and_then(|path| load_image_as_data_url(path))
                        .or_else(|| item.inline_data.as_ref().map(|data| {
                            format!("data:{};base64,{}", item.mime_type, data)
                        }));

                    ProofArtifactStateItem {
                        artifact_hash: item.artifact_hash.clone(),
                        name: item.name.clone(),
                        mime_type: item.mime_type.clone(),
                        size: item.size,
                        data_url,
                        dimensions: item.dimensions,
                        caption: item.caption.clone(),
                    }
                }).collect();

                // Add chat message
                let content = if narrative_preview.is_empty() {
                    format!("Submitted proof folder ({} files)", artifact_count)
                } else {
                    format!("Submitted proof: {}", narrative_preview)
                };

                let id = format!("folder-{}-{}", folder_id, tick);
                let msg = ChatMessage::new(
                    id,
                    *tick,
                    claimant.clone(),
                    content,
                    ChatMessageType::ProofFolderSubmitted {
                        quest_id: quest_id.clone(),
                        folder_id: folder_id.clone(),
                        artifact_count: *artifact_count,
                        narrative_preview: narrative_preview.clone(),
                        quest_title: title,
                        narrative: narrative.clone(),
                        artifacts: state_artifacts,
                    },
                );
                self.add_realm_message(realm_id, msg.clone());
                self.add_global_message(msg);
            }

            StreamEvent::ChatImage {
                tick,
                member,
                mime_type,
                data,
                artifact_hash,
                filename,
                dimensions,
                alt_text,
                asset_path,
                message_id,
            } => {
                let id = message_id.clone()
                    .unwrap_or_else(|| format!("img-{}-{}", tick, member));

                let content = alt_text.clone()
                    .or_else(|| filename.clone())
                    .unwrap_or_else(|| "[Image]".to_string());

                let msg = ChatMessage::new(
                    id,
                    *tick,
                    member.clone(),
                    content,
                    ChatMessageType::Image {
                        mime_type: mime_type.clone(),
                        inline_data: data.clone(),
                        artifact_hash: artifact_hash.clone(),
                        filename: filename.clone(),
                        dimensions: *dimensions,
                        alt_text: alt_text.clone(),
                        asset_path: asset_path.clone(),
                    },
                );
                self.add_global_message(msg);
            }

            StreamEvent::ChatGallery {
                tick,
                member,
                folder_id,
                title,
                items,
                message_id,
            } => {
                let id = message_id.clone()
                    .unwrap_or_else(|| format!("gallery-{}-{}", folder_id, tick));

                let content = title.clone()
                    .unwrap_or_else(|| format!("[Gallery: {} items]", items.len()));

                // Convert event items to state items
                let state_items: Vec<GalleryStateItem> = items.iter().map(|item| {
                    GalleryStateItem {
                        name: item.name.clone(),
                        mime_type: item.mime_type.clone(),
                        size: item.size,
                        thumbnail_data: item.thumbnail_data.clone(),
                        text_preview: item.text_preview.clone(),
                        artifact_hash: item.artifact_hash.clone(),
                        dimensions: item.dimensions,
                        asset_path: item.asset_path.clone(),
                    }
                }).collect();

                let msg = ChatMessage::new(
                    id,
                    *tick,
                    member.clone(),
                    content,
                    ChatMessageType::Gallery {
                        folder_id: folder_id.clone(),
                        title: title.clone(),
                        items: state_items,
                    },
                );
                self.add_global_message(msg);
            }

            StreamEvent::TokenMinted {
                tick,
                realm_id,
                token_id,
                steward,
                value_millis,
                blesser,
                ..
            } => {
                let duration = format_duration(*value_millis);
                let id = format!("token-minted-{}-{}", token_id, tick);
                let msg = ChatMessage::new(
                    id,
                    *tick,
                    blesser.clone(),
                    format!("Minted a Token of Gratitude ({}) for {}", duration, steward),
                    ChatMessageType::Text,
                );
                self.add_realm_message(realm_id, msg.clone());
                self.add_global_message(msg);
            }

            StreamEvent::GratitudePledged {
                tick,
                realm_id,
                token_id,
                pledger,
                target_quest_id,
                amount_millis,
            } => {
                let duration = format_duration(*amount_millis);
                let id = format!("gratitude-pledged-{}-{}", token_id, tick);
                let msg = ChatMessage::new(
                    id,
                    *tick,
                    pledger.clone(),
                    format!("Pledged {} of gratitude to quest {}", duration, target_quest_id),
                    ChatMessageType::Text,
                );
                self.add_realm_message(realm_id, msg.clone());
                self.add_global_message(msg);
            }

            StreamEvent::GratitudeReleased {
                tick,
                realm_id,
                token_id,
                from_steward,
                to_steward,
                target_quest_id,
                amount_millis,
            } => {
                let duration = format_duration(*amount_millis);
                let id = format!("gratitude-released-{}-{}", token_id, tick);
                let msg = ChatMessage::new(
                    id,
                    *tick,
                    from_steward.clone(),
                    format!("Released {} of gratitude to {} for quest {}", duration, to_steward, target_quest_id),
                    ChatMessageType::Text,
                );
                self.add_realm_message(realm_id, msg.clone());
                self.add_global_message(msg);
            }

            StreamEvent::GratitudeWithdrawn {
                tick,
                realm_id,
                token_id,
                steward,
                target_quest_id,
                amount_millis,
            } => {
                let duration = format_duration(*amount_millis);
                let id = format!("gratitude-withdrawn-{}-{}", token_id, tick);
                let msg = ChatMessage::new(
                    id,
                    *tick,
                    steward.clone(),
                    format!("Withdrew {} of gratitude from quest {}", duration, target_quest_id),
                    ChatMessageType::Text,
                );
                self.add_realm_message(realm_id, msg.clone());
                self.add_global_message(msg);
            }

            _ => {}
        }
    }

    fn add_realm_message(&mut self, realm_id: &str, msg: ChatMessage) {
        let messages = self
            .messages_by_realm
            .entry(realm_id.to_string())
            .or_default();
        messages.push_front(msg);
        while messages.len() > MAX_CHAT_MESSAGES {
            messages.pop_back();
        }
        self.total_messages += 1;
    }

    fn add_global_message(&mut self, msg: ChatMessage) {
        self.global_messages.push_front(msg);
        while self.global_messages.len() > MAX_CHAT_MESSAGES {
            self.global_messages.pop_back();
        }
    }

    /// Get recent messages across all realms (oldest first for chat display).
    pub fn recent_messages(&self, limit: usize) -> Vec<&ChatMessage> {
        // Take last N messages and return in chronological order (oldest first)
        // so newest messages appear at the bottom of the chat
        let msgs: Vec<_> = self.global_messages.iter().take(limit).collect();
        msgs.into_iter().rev().collect()
    }

    /// Get messages for a specific realm.
    pub fn messages_for_realm(&self, realm_id: &str) -> Vec<&ChatMessage> {
        self.messages_by_realm
            .get(realm_id)
            .map(|m| m.iter().collect())
            .unwrap_or_default()
    }

    /// Get a message by ID from global messages.
    pub fn get_message(&self, message_id: &str) -> Option<&ChatMessage> {
        self.global_messages.iter().find(|m| m.id == message_id)
    }

    /// Edit a message by ID.
    ///
    /// Saves the current version to history and updates content.
    pub fn edit_message(&mut self, message_id: &str, new_content: String, tick: u32) {
        // Edit in global messages
        if let Some(msg) = self.global_messages.iter_mut().find(|m| m.id == message_id) {
            if !msg.is_deleted && msg.content != new_content {
                msg.versions.push(MessageVersion {
                    content: msg.content.clone(),
                    edited_at: tick,
                });
                msg.content = new_content.clone();
            }
        }

        // Also edit in realm messages
        for messages in self.messages_by_realm.values_mut() {
            if let Some(msg) = messages.iter_mut().find(|m| m.id == message_id) {
                if !msg.is_deleted && msg.content != new_content {
                    msg.versions.push(MessageVersion {
                        content: msg.content.clone(),
                        edited_at: tick,
                    });
                    msg.content = new_content.clone();
                }
            }
        }
    }

    /// Delete a message by ID (soft delete with history preserved).
    pub fn delete_message(&mut self, message_id: &str, tick: u32) {
        // Delete in global messages
        if let Some(msg) = self.global_messages.iter_mut().find(|m| m.id == message_id) {
            if !msg.is_deleted {
                msg.versions.push(MessageVersion {
                    content: msg.content.clone(),
                    edited_at: tick,
                });
                msg.content.clear();
                msg.is_deleted = true;
            }
        }

        // Also delete in realm messages
        for messages in self.messages_by_realm.values_mut() {
            if let Some(msg) = messages.iter_mut().find(|m| m.id == message_id) {
                if !msg.is_deleted {
                    msg.versions.push(MessageVersion {
                        content: msg.content.clone(),
                        edited_at: tick,
                    });
                    msg.content.clear();
                    msg.is_deleted = true;
                }
            }
        }
    }

    /// Get blessing info for a proof.
    pub fn blessing_info(&self, quest_id: &str, claimant: &str) -> Option<&ProofBlessingInfo> {
        self.proof_blessings.get(&(quest_id.to_string(), claimant.to_string()))
    }

    /// Get all proofs sorted by tick (newest first).
    pub fn recent_proofs(&self, limit: usize) -> Vec<&ProofBlessingInfo> {
        let mut proofs: Vec<_> = self.proof_blessings.values().collect();
        proofs.sort_by(|a, b| b.tick.cmp(&a.tick));
        proofs.into_iter().take(limit).collect()
    }
}

/// Format milliseconds as human-readable duration.
fn format_duration(millis: u64) -> String {
    let seconds = millis / 1000;
    let minutes = seconds / 60;
    let hours = minutes / 60;

    if hours > 0 {
        let remaining_mins = minutes % 60;
        if remaining_mins > 0 {
            format!("{}h {}m", hours, remaining_mins)
        } else {
            format!("{}h", hours)
        }
    } else if minutes > 0 {
        format!("{}m", minutes)
    } else {
        format!("{}s", seconds)
    }
}

// Re-export from indras-ui
pub(crate) use indras_ui::load_image_as_data_url;
