//! Realm - collaborative space for N-peer communication.
//!
//! A Realm wraps an N-peer interface and provides a high-level API
//! for messaging, documents, and artifact sharing.

use crate::artifact::{Artifact, ArtifactDownload, ArtifactId, DownloadProgress};
use crate::document::Document;
use crate::error::{IndraError, Result};
use crate::invite::InviteCode;
use crate::member::{Member, MemberEvent, MemberId, MemberInfo};
use crate::message::{Content, Message, MessageId, MessagePayload};
use crate::network::RealmId;
use crate::quest::{Quest, QuestDocument, QuestId};
use crate::stream::broadcast_to_stream;

use chrono::Utc;
use futures::Stream;
use indras_core::{InterfaceEvent, MembershipChange};
use indras_node::{IndrasNode, ReceivedEvent};
use indras_storage::ContentRef;
use indras_transport::IrohIdentity;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::watch;
use tracing::debug;

/// Event type identifier for artifact sharing events.
const ARTIFACT_EVENT_TYPE: &str = "artifact:shared";

/// Metadata about a shared artifact, serialized as Custom event payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactMetadata {
    /// Content hash (BLAKE3).
    pub id: ArtifactId,
    /// Original filename.
    pub name: String,
    /// Size in bytes.
    pub size: u64,
    /// MIME type if known.
    pub mime_type: Option<String>,
}

/// A collaborative realm.
///
/// Realms are shared spaces where members can send messages,
/// collaborate on documents, and share artifacts.
///
/// # Example
///
/// ```ignore
/// // Create a realm
/// let realm = network.create_realm("Project Alpha").await?;
///
/// // Send a message
/// realm.send("Hello, team!").await?;
///
/// // Listen for messages
/// let mut messages = realm.messages();
/// while let Some(msg) = messages.next().await {
///     println!("{}: {}", msg.sender.name(), msg.content.as_text().unwrap_or(""));
/// }
/// ```
pub struct Realm {
    /// The realm ID.
    id: RealmId,
    /// Human-readable name.
    name: Option<String>,
    /// The invite code for this realm.
    invite: Option<InviteCode>,
    /// Reference to the underlying node.
    node: Arc<IndrasNode>,
}

impl Realm {
    /// Create a new realm wrapper.
    pub(crate) fn new(
        id: RealmId,
        name: Option<String>,
        invite: InviteCode,
        node: Arc<IndrasNode>,
    ) -> Self {
        Self {
            id,
            name,
            invite: Some(invite),
            node,
        }
    }

    /// Create a realm from just an ID (used when loading existing realms).
    pub(crate) fn from_id(id: RealmId, name: Option<String>, node: Arc<IndrasNode>) -> Self {
        Self {
            id,
            name,
            invite: None,
            node,
        }
    }

    // ============================================================
    // Properties
    // ============================================================

    /// Get the realm's unique identifier.
    pub fn id(&self) -> RealmId {
        self.id
    }

    /// Get the realm's human-readable name.
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// Get the invite code for this realm.
    ///
    /// Share this code with others to let them join.
    pub fn invite_code(&self) -> Option<&InviteCode> {
        self.invite.as_ref()
    }

    // ============================================================
    // Messaging
    // ============================================================

    /// Send a message to the realm.
    ///
    /// # Arguments
    ///
    /// * `content` - The message content (can be a string or Content enum)
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Send text
    /// realm.send("Hello!").await?;
    ///
    /// // Send with explicit Content
    /// realm.send(Content::Text("Hello!".into())).await?;
    /// ```
    pub async fn send(&self, content: impl Into<Content>) -> Result<MessageId> {
        let content = content.into();
        let payload = MessagePayload::new(content);
        let bytes = serialize_payload(&payload)?;

        let event_id = self.node.send_message(&self.id, bytes).await?;

        Ok(MessageId::new(self.id, event_id))
    }

    /// Send a reply to another message.
    ///
    /// # Arguments
    ///
    /// * `reply_to` - The message ID to reply to
    /// * `content` - The reply content
    pub async fn reply(
        &self,
        reply_to: MessageId,
        content: impl Into<Content>,
    ) -> Result<MessageId> {
        let content = content.into();
        let payload = MessagePayload::reply(content, reply_to);
        let bytes = serialize_payload(&payload)?;

        let event_id = self.node.send_message(&self.id, bytes).await?;

        Ok(MessageId::new(self.id, event_id))
    }

    /// Get a stream of incoming messages.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let mut messages = realm.messages();
    /// while let Some(msg) = messages.next().await {
    ///     println!("{}: {}", msg.sender.name(), msg.content.as_text().unwrap_or(""));
    /// }
    /// ```
    pub fn messages(&self) -> impl Stream<Item = Message> + Send + '_ {
        let rx = self.node.events(&self.id).ok();
        let realm_id = self.id;

        async_stream::stream! {
            if let Some(rx) = rx {
                let mut stream = broadcast_to_stream(rx);
                use futures::StreamExt;

                while let Some(event) = stream.next().await {
                    if let Some(msg) = convert_event_to_message(event, realm_id) {
                        yield msg;
                    }
                }
            }
        }
    }

    /// Get messages since a specific sequence number.
    pub async fn messages_since(&self, since: u64) -> Result<Vec<Message>> {
        let events = self.node.events_since(&self.id, since).await?;
        let realm_id = self.id;

        Ok(events
            .into_iter()
            .filter_map(|event| {
                // Convert InterfaceEvent to ReceivedEvent for conversion
                let received = ReceivedEvent {
                    interface_id: realm_id,
                    event,
                };
                convert_event_to_message(received, realm_id)
            })
            .collect())
    }

    // ============================================================
    // Members
    // ============================================================

    /// Get a stream of member events (joins, leaves, updates).
    ///
    /// This stream includes both CRDT-based membership changes and
    /// gossip-based discovery events. Use this to react to members
    /// joining or leaving the realm in real-time.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let mut events = realm.member_events();
    /// while let Some(event) = events.next().await {
    ///     match event {
    ///         MemberEvent::Joined(member) => println!("{} joined", member.name()),
    ///         MemberEvent::Left(member) => println!("{} left", member.name()),
    ///         MemberEvent::Discovered(info) => println!("Discovered {} via gossip", info.member.name()),
    ///         _ => {}
    ///     }
    /// }
    /// ```
    pub fn member_events(&self) -> impl Stream<Item = MemberEvent> + Send + '_ {
        let rx = self.node.events(&self.id).ok();

        async_stream::stream! {
            if let Some(rx) = rx {
                let mut stream = broadcast_to_stream(rx);
                use futures::StreamExt;

                while let Some(event) = stream.next().await {
                    if let Some(member_event) = convert_event_to_member_event(event) {
                        yield member_event;
                    }
                }
            }
        }
    }

    /// Get a stream of member events (alias for member_events).
    #[deprecated(since = "0.1.0", note = "Use member_events() instead")]
    pub fn members(&self) -> impl Stream<Item = MemberEvent> + Send + '_ {
        self.member_events()
    }

    /// Get the current list of realm members.
    ///
    /// Returns all known members including those discovered via gossip.
    /// Use `member_list_with_info()` to get PQ keys for secure communication.
    pub async fn member_list(&self) -> Result<Vec<Member>> {
        let identities: Vec<IrohIdentity> = self.node.members(&self.id).await?;

        Ok(identities.into_iter().map(Member::new).collect())
    }

    /// Get realm members with extended info including PQ keys.
    ///
    /// Returns member information discovered via gossip, including
    /// post-quantum cryptographic keys for secure direct communication.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let members = realm.member_list_with_info().await?;
    /// for info in members {
    ///     println!("{}: has PQ keys = {}", info.member.name(), info.has_pq_keys());
    /// }
    /// ```
    pub async fn member_list_with_info(&self) -> Result<Vec<MemberInfo>> {
        let peer_infos = self.node.members_with_info(&self.id).await?;

        Ok(peer_infos
            .into_iter()
            .map(MemberInfo::from_realm_peer_info)
            .collect())
    }

    /// Get the number of members in the realm.
    pub async fn member_count(&self) -> Result<usize> {
        Ok(self.node.members(&self.id).await?.len())
    }

    // ============================================================
    // Documents
    // ============================================================

    /// Get or create a typed document in this realm.
    ///
    /// Documents are CRDT-backed data structures that automatically
    /// synchronize across all realm members.
    ///
    /// # Type Parameters
    ///
    /// * `T` - The document type (must implement `DocumentSchema`)
    ///
    /// # Arguments
    ///
    /// * `name` - Unique name for the document within this realm
    ///
    /// # Example
    ///
    /// ```ignore
    /// #[derive(Default, Clone, Serialize, Deserialize, DocumentSchema)]
    /// struct QuestLog {
    ///     quests: Vec<Quest>,
    /// }
    ///
    /// let doc = realm.document::<QuestLog>("quests").await?;
    ///
    /// // Read current state
    /// let quests = doc.read();
    ///
    /// // Make changes (auto-synced)
    /// doc.update(|q| q.quests.push(Quest::new("Defeat dragon"))).await?;
    /// ```
    pub async fn document<T: crate::document::DocumentSchema>(
        &self,
        name: &str,
    ) -> Result<Document<T>> {
        Document::new(self.id, name.to_string(), Arc::clone(&self.node)).await
    }

    // ============================================================
    // Quests
    // ============================================================

    /// Get the quests document for this realm.
    ///
    /// Returns a CRDT-synchronized quest list that all realm members share.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let quests = realm.quests().await?;
    /// let open = quests.read().await.open_quests();
    /// println!("Open quests: {}", open.len());
    /// ```
    pub async fn quests(&self) -> Result<Document<QuestDocument>> {
        self.document::<QuestDocument>("quests").await
    }

    /// Create a new quest in this realm.
    ///
    /// # Arguments
    ///
    /// * `title` - Short title describing the quest
    /// * `description` - Detailed description of what needs to be done
    /// * `image` - Optional artifact ID for an image
    /// * `creator` - The member ID of the quest creator
    ///
    /// # Example
    ///
    /// ```ignore
    /// let quest_id = realm.create_quest(
    ///     "Review design doc",
    ///     "Please review the PDF and leave comments",
    ///     None,
    ///     my_id,
    /// ).await?;
    /// ```
    pub async fn create_quest(
        &self,
        title: impl Into<String>,
        description: impl Into<String>,
        image: Option<ArtifactId>,
        creator: MemberId,
    ) -> Result<QuestId> {
        let quest = Quest::new(title, description, image, creator);
        let quest_id = quest.id;

        let doc = self.quests().await?;
        doc.update(|d| {
            d.add(quest);
        })
        .await?;

        Ok(quest_id)
    }

    /// Claim a quest for a member.
    ///
    /// Once claimed, the quest shows who is working on it.
    ///
    /// # Arguments
    ///
    /// * `quest_id` - The quest to claim
    /// * `doer` - The member claiming the quest
    pub async fn claim_quest(&self, quest_id: QuestId, doer: MemberId) -> Result<()> {
        let doc = self.quests().await?;
        doc.update(|d| {
            if let Some(quest) = d.find_mut(&quest_id) {
                let _ = quest.claim(doer);
            }
        })
        .await?;

        Ok(())
    }

    /// Mark a quest as complete.
    ///
    /// # Arguments
    ///
    /// * `quest_id` - The quest to complete
    pub async fn complete_quest(&self, quest_id: QuestId) -> Result<()> {
        let doc = self.quests().await?;
        doc.update(|d| {
            if let Some(quest) = d.find_mut(&quest_id) {
                let _ = quest.complete();
            }
        })
        .await?;

        Ok(())
    }

    /// Unclaim a quest (release it back to open status).
    ///
    /// # Arguments
    ///
    /// * `quest_id` - The quest to unclaim
    pub async fn unclaim_quest(&self, quest_id: QuestId) -> Result<()> {
        let doc = self.quests().await?;
        doc.update(|d| {
            if let Some(quest) = d.find_mut(&quest_id) {
                let _ = quest.unclaim();
            }
        })
        .await?;

        Ok(())
    }

    // ============================================================
    // Artifacts
    // ============================================================

    /// Share a file as an artifact.
    ///
    /// Reads the file, hashes it with BLAKE3, stores it in blob storage,
    /// and broadcasts the artifact metadata to all realm members.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the file to share
    ///
    /// # Example
    ///
    /// ```ignore
    /// let artifact = realm.share_artifact("./document.pdf").await?;
    /// println!("Shared: {} ({} bytes)", artifact.name, artifact.size);
    /// ```
    pub async fn share_artifact(&self, path: impl AsRef<Path>) -> Result<Artifact> {
        let path = path.as_ref();

        // Read the file
        let file_data = tokio::fs::read(path)
            .await
            .map_err(|e| IndraError::Artifact(format!("Failed to read file: {}", e)))?;

        // Get filename
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unnamed")
            .to_string();

        // Compute BLAKE3 hash for artifact ID
        let hash = blake3::hash(&file_data);
        let id: ArtifactId = *hash.as_bytes();

        // Get file size
        let size = file_data.len() as u64;

        // Guess MIME type from extension
        let mime_type = path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| guess_mime_type(ext));

        // Store in blob storage via the node's storage
        // Note: We use our computed BLAKE3 hash as the artifact ID, not the content_ref's hash
        // (they should be identical since BlobStore also uses BLAKE3)
        let _content_ref = self
            .node
            .storage()
            .store_blob(&file_data)
            .await
            .map_err(|e| IndraError::Artifact(format!("Failed to store blob: {}", e)))?;

        debug!(
            artifact_id = %hex::encode(&id[..8]),
            name = %name,
            size = size,
            "Stored artifact in blob storage"
        );

        // Get our identity for the sharer field
        let sharer = Member::new(*self.node.identity());

        // Create artifact metadata for broadcasting
        let metadata = ArtifactMetadata {
            id,
            name: name.clone(),
            size,
            mime_type: mime_type.clone(),
        };

        // Serialize metadata
        let metadata_bytes = postcard::to_allocvec(&metadata)
            .map_err(|e| IndraError::Artifact(format!("Failed to serialize metadata: {}", e)))?;

        // Create and send a Custom event to announce the artifact
        let event = InterfaceEvent::<IrohIdentity>::custom(
            *self.node.identity(),
            self.node
                .events_since(&self.id, 0)
                .await
                .map(|e| e.len() as u64 + 1)
                .unwrap_or(1),
            ARTIFACT_EVENT_TYPE.to_string(),
            metadata_bytes,
        );

        // Send the event via the node (this will broadcast to other members)
        // We serialize and send as a message since there's no direct custom event API
        let event_bytes = postcard::to_allocvec(&event)
            .map_err(|e| IndraError::Artifact(format!("Failed to serialize event: {}", e)))?;
        let _ = self.node.send_message(&self.id, event_bytes).await;

        // Create and return the Artifact
        let artifact = Artifact {
            id,
            name,
            size,
            mime_type,
            sharer,
            shared_at: Utc::now(),
        };

        Ok(artifact)
    }

    /// Download a shared artifact.
    ///
    /// Fetches the artifact content from blob storage and provides
    /// a progress-tracking handle for the download.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let download = realm.download(&artifact).await?;
    ///
    /// // Track progress
    /// let mut progress = download.progress();
    /// while let Some(p) = progress.next().await {
    ///     println!("{}%", p.percent() as u32);
    ///     if p.is_complete() {
    ///         break;
    ///     }
    /// }
    ///
    /// // Get the downloaded file path
    /// let path = download.finish().await?;
    /// ```
    pub async fn download(&self, artifact: &Artifact) -> Result<ArtifactDownload> {
        // Create a content reference from the artifact ID
        let content_ref = ContentRef::new(artifact.id, artifact.size);

        // Determine destination path (use temp directory with artifact name)
        let destination = std::env::temp_dir().join(&artifact.name);

        // Fetch the blob from storage (this is local storage, so it's fast)
        let data = self
            .node
            .storage()
            .resolve_blob(&content_ref)
            .await
            .map_err(|e| IndraError::Artifact(format!("Failed to fetch artifact: {}", e)))?;

        // Write to destination file
        tokio::fs::write(&destination, &data)
            .await
            .map_err(|e| IndraError::Artifact(format!("Failed to write artifact to disk: {}", e)))?;

        // Create progress channel showing completion
        let (_progress_tx, progress_rx) = watch::channel(DownloadProgress {
            bytes_downloaded: artifact.size,
            total_bytes: artifact.size,
        });

        // Create the download handle (already complete)
        let (download, _cancel_rx) = ArtifactDownload::new(artifact.clone(), progress_rx, destination);

        Ok(download)
    }

    /// Get a stream of shared artifacts.
    ///
    /// Returns a stream that yields artifacts as they are shared in this realm.
    /// This listens for Custom events with the artifact event type.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let mut artifacts = realm.artifacts();
    /// while let Some(artifact) = artifacts.next().await {
    ///     println!("New artifact: {} ({} bytes)", artifact.name, artifact.size);
    /// }
    /// ```
    pub fn artifacts(&self) -> impl Stream<Item = Artifact> + Send + '_ {
        let rx = self.node.events(&self.id).ok();

        async_stream::stream! {
            if let Some(rx) = rx {
                let mut stream = broadcast_to_stream(rx);
                use futures::StreamExt;

                while let Some(event) = stream.next().await {
                    if let Some(artifact) = convert_event_to_artifact(event) {
                        yield artifact;
                    }
                }
            }
        }
    }

    // ============================================================
    // Escape hatches
    // ============================================================

    /// Access the underlying node.
    pub fn node(&self) -> &IndrasNode {
        &self.node
    }
}

impl Clone for Realm {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            name: self.name.clone(),
            invite: self.invite.clone(),
            node: Arc::clone(&self.node),
        }
    }
}

impl std::fmt::Debug for Realm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Realm")
            .field("id", &hex::encode(&self.id.as_bytes()[..8]))
            .field("name", &self.name)
            .finish()
    }
}

// ============================================================
// Helper functions
// ============================================================

fn serialize_payload(payload: &MessagePayload) -> Result<Vec<u8>> {
    postcard::to_allocvec(payload).map_err(IndraError::from)
}

fn convert_event_to_message(event: ReceivedEvent, realm_id: RealmId) -> Option<Message> {
    // Match on the InterfaceEvent enum to extract message data
    match &event.event {
        InterfaceEvent::Message {
            id,
            sender,
            content,
            timestamp,
        } => {
            let member = Member::new(*sender);
            let msg_id = MessageId::new(realm_id, *id);

            // Try to deserialize as MessagePayload first (new format with reply support)
            if let Ok(payload) = postcard::from_bytes::<MessagePayload>(content) {
                if let Some(reply_to) = payload.reply_to {
                    return Some(Message::reply(msg_id, member, payload.content, *timestamp, reply_to));
                } else {
                    return Some(Message::new(msg_id, member, payload.content, *timestamp));
                }
            }

            // Fall back to deserializing as plain Content (legacy format)
            let msg_content: Content = postcard::from_bytes(content).ok()?;
            Some(Message::new(msg_id, member, msg_content, *timestamp))
        }
        InterfaceEvent::Custom {
            id,
            sender,
            payload,
            timestamp,
            ..
        } => {
            let member = Member::new(*sender);
            let msg_id = MessageId::new(realm_id, *id);

            // Try to deserialize as MessagePayload first (new format with reply support)
            if let Ok(msg_payload) = postcard::from_bytes::<MessagePayload>(payload) {
                if let Some(reply_to) = msg_payload.reply_to {
                    return Some(Message::reply(msg_id, member, msg_payload.content, *timestamp, reply_to));
                } else {
                    return Some(Message::new(msg_id, member, msg_payload.content, *timestamp));
                }
            }

            // Fall back to deserializing as plain Content (legacy format)
            let msg_content: Content = postcard::from_bytes(payload).ok()?;
            Some(Message::new(msg_id, member, msg_content, *timestamp))
        }
        _ => None, // Other event types are not messages
    }
}

fn convert_event_to_member_event(event: ReceivedEvent) -> Option<MemberEvent> {
    match &event.event {
        InterfaceEvent::MembershipChange { change, .. } => {
            match change {
                MembershipChange::Joined { peer } => {
                    Some(MemberEvent::Joined(Member::new(*peer)))
                }
                MembershipChange::Left { peer } => {
                    Some(MemberEvent::Left(Member::new(*peer)))
                }
                MembershipChange::Created { creator } => {
                    // Treat creator as joining the realm
                    Some(MemberEvent::Joined(Member::new(*creator)))
                }
                MembershipChange::Invited { peer, .. } => {
                    // Treat invited peer as joining
                    Some(MemberEvent::Joined(Member::new(*peer)))
                }
                MembershipChange::Removed { peer, .. } => {
                    // Treat removed peer as leaving
                    Some(MemberEvent::Left(Member::new(*peer)))
                }
            }
        }
        _ => None, // Other event types are not member events
    }
}

fn convert_event_to_artifact(event: ReceivedEvent) -> Option<Artifact> {
    match &event.event {
        InterfaceEvent::Custom {
            sender,
            event_type,
            payload,
            timestamp,
            ..
        } => {
            // Check if this is an artifact event
            if event_type != ARTIFACT_EVENT_TYPE {
                return None;
            }

            // Try to deserialize the artifact metadata
            let metadata: ArtifactMetadata = postcard::from_bytes(payload).ok()?;

            // Create the artifact
            Some(Artifact {
                id: metadata.id,
                name: metadata.name,
                size: metadata.size,
                mime_type: metadata.mime_type,
                sharer: Member::new(*sender),
                shared_at: *timestamp,
            })
        }
        _ => None,
    }
}

/// Guess MIME type from file extension.
fn guess_mime_type(ext: &str) -> String {
    match ext.to_lowercase().as_str() {
        // Text
        "txt" => "text/plain",
        "html" | "htm" => "text/html",
        "css" => "text/css",
        "js" => "text/javascript",
        "json" => "application/json",
        "xml" => "application/xml",
        "csv" => "text/csv",
        "md" => "text/markdown",
        // Images
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        "bmp" => "image/bmp",
        // Audio
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "ogg" => "audio/ogg",
        "flac" => "audio/flac",
        "aac" => "audio/aac",
        // Video
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "avi" => "video/x-msvideo",
        "mov" => "video/quicktime",
        "mkv" => "video/x-matroska",
        // Documents
        "pdf" => "application/pdf",
        "doc" => "application/msword",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "xls" => "application/vnd.ms-excel",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "ppt" => "application/vnd.ms-powerpoint",
        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        // Archives
        "zip" => "application/zip",
        "tar" => "application/x-tar",
        "gz" | "gzip" => "application/gzip",
        "rar" => "application/vnd.rar",
        "7z" => "application/x-7z-compressed",
        // Code
        "rs" => "text/x-rust",
        "py" => "text/x-python",
        "go" => "text/x-go",
        "java" => "text/x-java",
        "c" => "text/x-c",
        "cpp" | "cc" | "cxx" => "text/x-c++",
        "h" | "hpp" => "text/x-c-header",
        "sh" => "application/x-sh",
        "toml" => "application/toml",
        "yaml" | "yml" => "application/x-yaml",
        // Fonts
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        // Default
        _ => "application/octet-stream",
    }
    .to_string()
}

// Simple hex encoding for debug output
mod hex {
    pub fn encode(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{:02x}", b)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Tests would require setting up a full node, which is complex
    // Integration tests are more appropriate
}
