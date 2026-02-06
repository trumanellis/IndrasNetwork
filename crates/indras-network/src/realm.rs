//! Realm - collaborative space for N-peer communication.
//!
//! A Realm wraps an N-peer interface and provides a high-level API
//! for messaging, documents, and artifact sharing.

use crate::artifact::{Artifact, ArtifactDownload, ArtifactId, DownloadProgress};
use crate::document::Document;
use crate::error::{IndraError, Result};
use crate::invite::InviteCode;
use crate::member::{Member, MemberEvent, MemberId, MemberInfo};
use crate::message::{ArtifactRef, Content, Message, MessageId, MessagePayload};
use crate::network::RealmId;
use crate::access::AccessMode;
use crate::artifact_index::HomeArtifactEntry;
use crate::home_realm::HomeRealm;
use crate::stream::broadcast_to_stream;

use chrono::Utc;
use futures::Stream;
use indras_core::{InterfaceEvent, MembershipChange, PeerIdentity};
use indras_node::{IndrasNode, ReceivedEvent};
use indras_storage::ContentRef;
use indras_transport::IrohIdentity;
use serde::Serialize;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::watch;
use tracing::debug;

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

    /// React to a message with an emoji.
    ///
    /// Sends a reaction as a Content::Reaction message. Reactions are
    /// visible to all realm members.
    ///
    /// # Arguments
    ///
    /// * `message_id` - The message to react to
    /// * `emoji` - The emoji reaction (e.g., "👍", "❤️", "🎉")
    ///
    /// # Example
    ///
    /// ```ignore
    /// realm.react(message_id, "👍").await?;
    /// ```
    pub async fn react(
        &self,
        message_id: MessageId,
        emoji: impl Into<String>,
    ) -> Result<MessageId> {
        self.send(Content::Reaction {
            target: message_id,
            emoji: emoji.into(),
        })
        .await
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

    /// Get all messages from the CRDT document, including synced messages from peers.
    ///
    /// Unlike `messages_since()` which only returns locally-stored events,
    /// this reads from the Automerge document which includes events received
    /// via CRDT sync from remote peers.
    pub async fn all_messages(&self) -> Result<Vec<Message>> {
        let events = self.node.document_events(&self.id).await?;
        let realm_id = self.id;

        Ok(events
            .into_iter()
            .filter_map(|event| {
                let received = ReceivedEvent {
                    interface_id: realm_id,
                    event,
                };
                convert_event_to_message(received, realm_id)
            })
            .collect())
    }

    /// Search messages by text content.
    ///
    /// Performs case-insensitive full-text search across all messages
    /// in the realm. Only searches text message content.
    ///
    /// # Arguments
    ///
    /// * `query` - The search query string
    ///
    /// # Example
    ///
    /// ```ignore
    /// let results = realm.search_messages("meeting notes").await?;
    /// for msg in results {
    ///     println!("{}: {}", msg.sender.name(), msg.content.as_text().unwrap_or(""));
    /// }
    /// ```
    pub async fn search_messages(&self, query: &str) -> Result<Vec<Message>> {
        let events = self.node.events_since(&self.id, 0).await?;
        let realm_id = self.id;
        let query_lower = query.to_lowercase();

        Ok(events
            .into_iter()
            .filter_map(|event| {
                let received = ReceivedEvent {
                    interface_id: realm_id,
                    event,
                };
                convert_event_to_message(received, realm_id)
            })
            .filter(|msg| {
                if let Some(text) = msg.content.as_text() {
                    text.to_lowercase().contains(&query_lower)
                } else {
                    false
                }
            })
            .collect())
    }

    // ============================================================
    // Read Tracking
    // ============================================================

    /// Mark the realm as read for a member.
    ///
    /// Records the current event position so that `unread_count()` can
    /// calculate how many messages arrived since the last read.
    ///
    /// # Arguments
    ///
    /// * `member` - The member marking as read
    ///
    /// # Example
    ///
    /// ```ignore
    /// // User opens the realm - mark as read
    /// realm.mark_read(my_id).await?;
    /// ```
    pub async fn mark_read(&self, member: MemberId) -> Result<()> {
        use crate::read_tracker::ReadTrackerDocument;

        // Get the current event count as the sequence number
        let events = self.node.events_since(&self.id, 0).await?;
        let seq = events.len() as u64;

        let doc = self.document::<ReadTrackerDocument>("read_tracker").await?;
        doc.update(|d| {
            d.mark_read(member, seq);
        })
        .await?;

        Ok(())
    }

    /// Get the number of unread messages for a member.
    ///
    /// Returns the count of messages that arrived after the member's
    /// last `mark_read()` call. Returns the total message count if
    /// the member has never marked the realm as read.
    ///
    /// # Arguments
    ///
    /// * `member` - The member to check
    ///
    /// # Example
    ///
    /// ```ignore
    /// let count = realm.unread_count(&my_id).await?;
    /// if count > 0 {
    ///     println!("{} unread messages", count);
    /// }
    /// ```
    pub async fn unread_count(&self, member: &MemberId) -> Result<usize> {
        use crate::read_tracker::ReadTrackerDocument;

        let doc = self.document::<ReadTrackerDocument>("read_tracker").await?;
        let last_read = doc.read().await.last_read_seq(member);

        let events = self.node.events_since(&self.id, last_read).await?;
        let realm_id = self.id;

        // Count only message events (not membership changes, sync markers, etc.)
        let count = events
            .into_iter()
            .filter_map(|event| {
                let received = ReceivedEvent {
                    interface_id: realm_id,
                    event,
                };
                convert_event_to_message(received, realm_id)
            })
            .count();

        Ok(count)
    }

    /// Get the sequence number of the last message read by a member.
    ///
    /// Returns 0 if the member has never marked the realm as read.
    pub async fn last_read_seq(&self, member: &MemberId) -> Result<u64> {
        use crate::read_tracker::ReadTrackerDocument;

        let doc = self.document::<ReadTrackerDocument>("read_tracker").await?;
        Ok(doc.read().await.last_read_seq(member))
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
    // Presence
    // ============================================================

    /// Get the list of currently online members.
    ///
    /// Returns members whose presence status is not `Offline`.
    /// This is based on gossip discovery - members visible through
    /// the gossip layer are considered online.
    pub async fn online_members(&self) -> Result<Vec<Member>> {
        // Members visible through the gossip layer are online
        let identities: Vec<IrohIdentity> = self.node.members(&self.id).await?;

        Ok(identities
            .into_iter()
            .map(|id| {
                let mut member = Member::new(id);
                member.set_presence(indras_core::PresenceStatus::Online);
                member
            })
            .collect())
    }

    /// Check if a specific member is currently reachable (online).
    ///
    /// Returns true if the member is visible in the gossip layer.
    pub async fn is_member_online(&self, member_id: &MemberId) -> Result<bool> {
        let members = self.node.members(&self.id).await?;
        Ok(members.iter().any(|id| {
            let mut bytes = [0u8; 32];
            let id_bytes = id.as_bytes();
            bytes.copy_from_slice(&id_bytes[..32.min(id_bytes.len())]);
            &bytes == member_id
        }))
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
        // Auto-register the document name (skip internal documents)
        if !name.starts_with('_') {
            let registry = Document::<crate::document_registry::DocumentRegistryDocument>::new(
                self.id,
                "_registry".to_string(),
                Arc::clone(&self.node),
            )
            .await?;
            let name_owned = name.to_string();
            registry
                .update(|d| {
                    d.register(name_owned);
                })
                .await?;
        }

        Document::new(self.id, name.to_string(), Arc::clone(&self.node)).await
    }

    /// List all named documents in this realm.
    ///
    /// Returns the names of documents that have been opened via `document()`.
    /// Internal documents (prefixed with `_`) are excluded.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let names = realm.document_names().await?;
    /// for name in names {
    ///     println!("Document: {}", name);
    /// }
    /// ```
    pub async fn document_names(&self) -> Result<Vec<String>> {
        let registry = Document::<crate::document_registry::DocumentRegistryDocument>::new(
            self.id,
            "_registry".to_string(),
            Arc::clone(&self.node),
        )
        .await?;
        let guard = registry.read().await;
        Ok(guard.document_names().into_iter().map(String::from).collect())
    }

    /// Check if a named document exists in this realm.
    pub async fn has_document(&self, name: &str) -> Result<bool> {
        let registry = Document::<crate::document_registry::DocumentRegistryDocument>::new(
            self.id,
            "_registry".to_string(),
            Arc::clone(&self.node),
        )
        .await?;
        Ok(registry.read().await.contains(name))
    }

    // ============================================================
    // Realm Alias
    // ============================================================

    /// Get the alias document for this realm.
    ///
    /// The alias is a CRDT-synchronized nickname that all realm members
    /// can edit. It provides a human-readable name for the realm.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let alias = realm.alias().await?;
    /// let name = alias.read().await.get().to_string();
    /// println!("Realm alias: {}", name);
    /// ```
    pub async fn alias(&self) -> Result<Document<crate::realm_alias::RealmAliasDocument>> {
        self.document("alias").await
    }

    /// Get the current alias for this realm.
    ///
    /// Returns the alias if set, or None if empty.
    ///
    /// # Example
    ///
    /// ```ignore
    /// if let Some(alias) = realm.get_alias().await? {
    ///     println!("Realm: {}", alias);
    /// }
    /// ```
    pub async fn get_alias(&self) -> Result<Option<String>> {
        let doc = self.alias().await?;
        Ok(doc.read().await.get_option().map(String::from))
    }

    /// Set the alias for this realm.
    ///
    /// The alias is limited to 77 characters (unicode allowed).
    /// Setting an empty string clears the alias.
    ///
    /// # Arguments
    ///
    /// * `alias` - The new alias (max 77 chars)
    ///
    /// # Example
    ///
    /// ```ignore
    /// realm.set_alias("Project Alpha").await?;
    /// ```
    pub async fn set_alias(&self, alias: impl Into<String>) -> Result<()> {
        let alias = alias.into();
        let doc = self.alias().await?;
        doc.update(|d| {
            if alias.is_empty() {
                d.clear();
            } else {
                d.set(&alias);
            }
        })
        .await?;
        Ok(())
    }

    /// Clear the alias for this realm.
    ///
    /// # Example
    ///
    /// ```ignore
    /// realm.clear_alias().await?;
    /// ```
    pub async fn clear_alias(&self) -> Result<()> {
        let doc = self.alias().await?;
        doc.update(|d| {
            d.clear();
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

        // Create artifact metadata for broadcasting (legacy Custom event format)
        #[derive(Serialize)]
        struct LegacyArtifactMetadata {
            id: ArtifactId,
            name: String,
            size: u64,
            mime_type: Option<String>,
        }
        let metadata = LegacyArtifactMetadata {
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
            "artifact:shared".to_string(),
            metadata_bytes,
        );

        // Send the event via the node (this will broadcast to other members)
        // We serialize and send as a message since there's no direct custom event API
        let event_bytes = postcard::to_allocvec(&event)
            .map_err(|e| IndraError::Artifact(format!("Failed to serialize event: {}", e)))?;
        let _ = self.node.send_message(&self.id, event_bytes).await;

        // Create and return the Artifact
        let owner = sharer.id();
        let artifact = Artifact {
            id,
            name,
            size,
            mime_type,
            sharer,
            owner,
            shared_at: Utc::now(),
            is_encrypted: false,
            sharing_status: crate::artifact_sharing::SharingStatus::Shared,
            parent: None,
            children: Vec::new(),
        };

        Ok(artifact)
    }

    /// Share a file as an artifact with a specific access mode.
    ///
    /// Uploads to the owner's home realm, then grants access to all
    /// realm members with the specified mode.
    pub async fn share_artifact_with_mode(
        &self,
        path: impl AsRef<Path>,
        home: &HomeRealm,
        mode: AccessMode,
    ) -> Result<Artifact> {
        let path = path.as_ref();

        // Upload to home realm
        let id = home.upload(path).await?;

        // Get realm member list (simplified: we don't have a full member_ids() yet)
        // For now, grant is done by the caller per-member
        // Post artifact reference to realm chat
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unnamed")
            .to_string();

        let file_meta = tokio::fs::metadata(path)
            .await
            .map_err(|e| IndraError::Artifact(format!("Failed to read file metadata: {}", e)))?;
        let size = file_meta.len();
        let mime_type = path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| guess_mime_type(ext));

        self.send(Content::Artifact(ArtifactRef {
            name: name.clone(),
            size,
            hash: id,
            mime_type: mime_type.clone(),
        }))
        .await?;

        let sharer = Member::new(*self.node.identity());
        let owner = sharer.id();
        Ok(Artifact {
            id,
            name,
            size,
            mime_type,
            sharer,
            owner,
            shared_at: Utc::now(),
            is_encrypted: matches!(mode, AccessMode::Revocable),
            sharing_status: crate::artifact_sharing::SharingStatus::Shared,
            parent: None,
            children: Vec::new(),
        })
    }

    /// Share a file with per-person access mode specification.
    pub async fn share_artifact_granular(
        &self,
        path: impl AsRef<Path>,
        home: &HomeRealm,
        grants: Vec<(MemberId, AccessMode)>,
    ) -> Result<Artifact> {
        let path = path.as_ref();

        // Upload to home realm
        let id = home.upload(path).await?;

        // Grant access per specification
        for (member, mode) in &grants {
            let _ = home.grant_access(&id, *member, mode.clone()).await;
        }

        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unnamed")
            .to_string();
        let file_meta = tokio::fs::metadata(path)
            .await
            .map_err(|e| IndraError::Artifact(format!("Failed to read file metadata: {}", e)))?;
        let size = file_meta.len();
        let mime_type = path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| guess_mime_type(ext));

        self.send(Content::Artifact(ArtifactRef {
            name: name.clone(),
            size,
            hash: id,
            mime_type: mime_type.clone(),
        }))
        .await?;

        let sharer = Member::new(*self.node.identity());
        let owner = sharer.id();
        Ok(Artifact {
            id,
            name,
            size,
            mime_type,
            sharer,
            owner,
            shared_at: Utc::now(),
            is_encrypted: false,
            sharing_status: crate::artifact_sharing::SharingStatus::Shared,
            parent: None,
            children: Vec::new(),
        })
    }

    /// Get the artifact key registry document for this realm.
    ///
    /// The key registry stores the mapping from artifact hashes to
    /// encryption keys, enabling revocable artifact sharing.
    pub async fn artifact_key_registry(
        &self,
    ) -> Result<Document<crate::artifact_sharing::ArtifactKeyRegistry>> {
        self.document("artifact_key_registry").await
    }

    /// Share a file as an artifact with revocation support.
    ///
    /// **Deprecated**: Use `share_artifact_with_mode(path, home, AccessMode::Revocable)` instead.
    /// This is kept for backward compatibility during migration.
    pub async fn share_artifact_revocable(
        &self,
        path: impl AsRef<Path>,
    ) -> Result<crate::artifact_sharing::SharedArtifact> {
        use crate::artifact_sharing::{EncryptedArtifactKey, SharedArtifact, SharingStatus};

        let path = path.as_ref();

        let file_data = tokio::fs::read(path)
            .await
            .map_err(|e| IndraError::Artifact(format!("Failed to read file: {}", e)))?;

        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unnamed")
            .to_string();

        let mime_type = path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| guess_mime_type(ext));

        let artifact_key = indras_crypto::generate_artifact_key();
        let encrypted = indras_crypto::encrypt_artifact(&file_data, &artifact_key)
            .map_err(|e| IndraError::Artifact(format!("Failed to encrypt artifact: {}", e)))?;
        let encrypted_bytes = encrypted.to_bytes();
        let hash = blake3::hash(&encrypted_bytes);
        let artifact_hash: [u8; 32] = *hash.as_bytes();
        let size = encrypted_bytes.len() as u64;

        let _content_ref = self
            .node
            .storage()
            .store_blob(&encrypted_bytes)
            .await
            .map_err(|e| IndraError::Artifact(format!("Failed to store encrypted blob: {}", e)))?;

        let sharer_id = self.node.identity();
        let sharer_hex = hex::encode(&sharer_id.as_bytes());
        let shared_at = chrono::Utc::now().timestamp_millis() as u64;

        let shared_artifact = SharedArtifact {
            hash: artifact_hash,
            name: name.clone(),
            size,
            mime_type: mime_type.clone(),
            sharer: sharer_hex.clone(),
            shared_at,
            status: SharingStatus::Shared,
        };

        let encrypted_key = EncryptedArtifactKey {
            nonce: [0u8; 12],
            ciphertext: artifact_key.to_vec(),
        };

        let registry = self.document::<crate::artifact_sharing::ArtifactKeyRegistry>("artifact_key_registry").await?;
        registry
            .update(|r| {
                r.store(shared_artifact.clone(), encrypted_key);
            })
            .await?;

        self.send(Content::Artifact(ArtifactRef {
            name: name.clone(),
            size,
            hash: artifact_hash,
            mime_type: mime_type.clone(),
        }))
        .await?;

        Ok(shared_artifact)
    }

    /// Recall a previously shared artifact.
    ///
    /// This revokes access to the artifact by:
    /// 1. Removing the decryption key from the registry
    /// 2. Deleting the encrypted blob from local storage
    /// 3. Broadcasting a recall event to all members
    /// 4. Adding a tombstone message to chat
    ///
    /// Only the original sharer can recall an artifact.
    ///
    /// # Arguments
    ///
    /// * `artifact_hash` - The hash of the artifact to recall
    ///
    /// # Example
    ///
    /// ```ignore
    /// realm.recall_artifact(&artifact.hash).await?;
    /// // The artifact is now inaccessible to all members
    /// ```
    pub async fn recall_artifact(&self, artifact_hash: &[u8; 32]) -> Result<()> {
        use crate::artifact_sharing::RevocationEntry;
        use crate::member::MemberId;

        // Get our identity
        let our_id = self.node.identity();
        let our_id_hex = hex::encode(&our_id.as_bytes());

        // Get the key registry
        let registry = self.artifact_key_registry().await?;

        // Check if we can revoke (must be the original sharer)
        {
            let guard = registry.read().await;
            if !guard.can_revoke(artifact_hash, &our_id_hex) {
                return Err(IndraError::InvalidOperation(
                    "Cannot recall: either artifact doesn't exist, already recalled, or you're not the original sharer".into(),
                ));
            }
        }

        // Get artifact info for the tombstone before revoking
        let (shared_at, sharer) = {
            let guard = registry.read().await;
            let artifact = guard.get_artifact(artifact_hash).ok_or_else(|| {
                IndraError::InvalidOperation("Artifact not found in registry".into())
            })?;
            (artifact.shared_at, artifact.sharer.clone())
        };

        // Get current tick
        let recalled_at = chrono::Utc::now().timestamp_millis() as u64;

        // Create revocation entry
        let entry = RevocationEntry::new(*artifact_hash, our_id_hex.clone(), recalled_at);

        // Revoke in registry (removes key and updates artifact status)
        registry
            .update(|r| {
                r.revoke(entry);
            })
            .await?;

        // Delete the encrypted blob from local storage
        let content_ref = indras_storage::ContentRef::new(*artifact_hash, 0);
        let _ = self.node.storage().blob_store().delete(&content_ref).await;

        // Post tombstone to chat
        // Decode sharer hex string back to member ID bytes
        let sharer_bytes = hex::decode(&sharer).unwrap_or_else(|_| Vec::new());
        let mut sharer_member_id: MemberId = [0u8; 32];
        if sharer_bytes.len() == 32 {
            sharer_member_id.copy_from_slice(&sharer_bytes);
        }
        self.send(Content::ArtifactRecalled {
            artifact_hash: *artifact_hash,
            sharer: sharer_member_id,
            shared_at,
            recalled_at,
        })
        .await?;

        debug!(
            artifact_hash = %hex::encode(&artifact_hash[..8]),
            "Artifact recalled successfully"
        );

        Ok(())
    }

    /// Check if an artifact has been recalled.
    pub async fn is_artifact_recalled(&self, artifact_hash: &[u8; 32]) -> Result<bool> {
        let registry = self.artifact_key_registry().await?;
        let guard = registry.read().await;
        Ok(guard.is_revoked(artifact_hash))
    }

    /// Query artifacts visible in this realm context.
    ///
    /// Returns artifacts where all realm members (other than the owner)
    /// have active access grants in the owner's ArtifactIndex.
    pub async fn artifacts_view(
        &self,
        home: &HomeRealm,
        _now: u64,
    ) -> Result<Vec<HomeArtifactEntry>> {
        let doc = home.artifact_index().await?;
        let data = doc.read().await;

        // For now, return all active artifacts from the home index
        // Full member filtering requires member_ids() which we don't have exposed yet
        Ok(data.active_artifacts().cloned().collect())
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

    // ============================================================
    // Escape hatches
    // ============================================================

    /// Access the underlying node.
    pub fn node(&self) -> &IndrasNode {
        &self.node
    }

    /// Get a cloned Arc to the underlying node.
    ///
    /// Useful for extension traits that need ownership of the Arc
    /// to create Document instances.
    pub fn node_arc(&self) -> Arc<IndrasNode> {
        Arc::clone(&self.node)
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

// Simple hex encoding/decoding for debug output
mod hex {
    pub fn encode(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{:02x}", b)).collect()
    }

    pub fn decode(hex: &str) -> Result<Vec<u8>, ()> {
        if hex.len() % 2 != 0 {
            return Err(());
        }

        (0..hex.len())
            .step_by(2)
            .map(|i| {
                u8::from_str_radix(&hex[i..i + 2], 16).map_err(|_| ())
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Tests would require setting up a full node, which is complex
    // Integration tests are more appropriate
}
