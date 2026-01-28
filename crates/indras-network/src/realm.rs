//! Realm - collaborative space for N-peer communication.
//!
//! A Realm wraps an N-peer interface and provides a high-level API
//! for messaging, documents, and artifact sharing.

use crate::artifact::{Artifact, ArtifactDownload, ArtifactId, DownloadProgress};
use crate::attention::{AttentionDocument, AttentionEventId, QuestAttention};
use crate::blessing::{Blessing, BlessingDocument, BlessingId, ClaimId};
use crate::document::Document;
use crate::error::{IndraError, Result};
use crate::invite::InviteCode;
use crate::member::{Member, MemberEvent, MemberId, MemberInfo};
use crate::message::{ArtifactRef, Content, Message, MessageId, MessagePayload};
use crate::network::RealmId;
use crate::note::{Note, NoteDocument, NoteId};
use crate::quest::{Quest, QuestDocument, QuestId};
use crate::stream::broadcast_to_stream;

use chrono::Utc;
use futures::Stream;
use indras_core::{InterfaceEvent, MembershipChange, PeerIdentity};
use indras_node::{IndrasNode, ReceivedEvent};
use indras_storage::ContentRef;
use indras_transport::IrohIdentity;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
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

    /// Submit a claim/proof of service for a quest.
    ///
    /// Members submit claims with optional proof artifacts to demonstrate
    /// they've completed work for the quest. Multiple members can submit
    /// claims for the same quest.
    ///
    /// # Arguments
    ///
    /// * `quest_id` - The quest to claim
    /// * `claimant` - The member submitting the claim
    /// * `proof` - Optional proof artifact (document, image, etc.)
    ///
    /// # Returns
    ///
    /// The index of the newly created claim.
    pub async fn submit_quest_claim(
        &self,
        quest_id: QuestId,
        claimant: MemberId,
        proof: Option<ArtifactId>,
    ) -> Result<usize> {
        let mut claim_index = 0;
        let doc = self.quests().await?;
        doc.update(|d| {
            if let Some(quest) = d.find_mut(&quest_id) {
                if let Ok(idx) = quest.submit_claim(claimant, proof) {
                    claim_index = idx;
                }
            }
        })
        .await?;

        Ok(claim_index)
    }

    /// Submit a quest claim with proof artifact and post to realm chat.
    ///
    /// This is the preferred method for submitting proofs as it automatically
    /// posts a ProofSubmitted message to the realm chat, notifying other
    /// members that proof is available for blessing.
    ///
    /// # Arguments
    ///
    /// * `quest_id` - The quest to claim
    /// * `claimant` - The member submitting the claim
    /// * `proof_artifact` - The artifact serving as proof (includes metadata)
    ///
    /// # Returns
    ///
    /// The index of the newly created claim.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Share proof artifact
    /// let artifact = realm.share_artifact("./completion_screenshot.png").await?;
    ///
    /// // Submit proof to quest and notify chat
    /// let artifact_ref = ArtifactRef {
    ///     name: artifact.name,
    ///     size: artifact.size,
    ///     hash: artifact.id,
    ///     mime_type: artifact.mime_type,
    /// };
    /// let claim_idx = realm.submit_quest_proof(quest_id, my_id, artifact_ref).await?;
    /// ```
    pub async fn submit_quest_proof(
        &self,
        quest_id: QuestId,
        claimant: MemberId,
        proof_artifact: ArtifactRef,
    ) -> Result<usize> {
        // Submit the claim with the artifact ID (hash)
        let claim_index = self
            .submit_quest_claim(quest_id, claimant, Some(proof_artifact.hash))
            .await?;

        // Post ProofSubmitted message to chat
        self.send(Content::ProofSubmitted {
            quest_id,
            claimant,
            artifact: proof_artifact,
        })
        .await?;

        Ok(claim_index)
    }

    /// Verify a claim on a quest.
    ///
    /// The quest creator should call this to verify that a claim is valid.
    /// Verified claims indicate the work was completed satisfactorily.
    ///
    /// # Arguments
    ///
    /// * `quest_id` - The quest containing the claim
    /// * `claim_index` - The index of the claim to verify
    pub async fn verify_quest_claim(&self, quest_id: QuestId, claim_index: usize) -> Result<()> {
        let doc = self.quests().await?;
        doc.update(|d| {
            if let Some(quest) = d.find_mut(&quest_id) {
                let _ = quest.verify_claim(claim_index);
            }
        })
        .await?;

        Ok(())
    }

    /// Mark a quest as complete.
    ///
    /// The quest creator should call this after verifying claims.
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

    /// Claim a quest for a member (legacy compatibility).
    ///
    /// This submits a claim without proof. For the new proof-of-service
    /// model, use `submit_quest_claim` instead.
    #[deprecated(since = "0.2.0", note = "Use submit_quest_claim() instead")]
    pub async fn claim_quest(&self, quest_id: QuestId, doer: MemberId) -> Result<()> {
        self.submit_quest_claim(quest_id, doer, None).await?;
        Ok(())
    }

    /// Unclaim a quest (legacy compatibility).
    ///
    /// In the proof-of-service model, claims cannot be removed once submitted.
    #[deprecated(since = "0.2.0", note = "Claims cannot be removed in proof-of-service model")]
    #[allow(deprecated)]
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
    // Notes
    // ============================================================

    /// Get the notes document for this realm.
    ///
    /// Returns a CRDT-synchronized note list that all realm members share.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let notes = realm.notes().await?;
    /// let all_notes = notes.read().await.notes_by_recent();
    /// println!("Total notes: {}", all_notes.len());
    /// ```
    pub async fn notes(&self) -> Result<Document<NoteDocument>> {
        self.document::<NoteDocument>("notes").await
    }

    /// Create a new note in this realm.
    ///
    /// # Arguments
    ///
    /// * `title` - Title of the note
    /// * `content` - Markdown content
    /// * `author` - The member ID of the note author
    /// * `tags` - Tags for organization
    ///
    /// # Example
    ///
    /// ```ignore
    /// let note_id = realm.create_note(
    ///     "Meeting Notes",
    ///     "# Project Update\n\n- Item 1\n- Item 2",
    ///     my_id,
    ///     vec!["work".into(), "meeting".into()],
    /// ).await?;
    /// ```
    pub async fn create_note(
        &self,
        title: impl Into<String>,
        content: impl Into<String>,
        author: MemberId,
        tags: Vec<String>,
    ) -> Result<NoteId> {
        let note = Note::with_tags(title, content, author, tags);
        let note_id = note.id;

        let doc = self.notes().await?;
        doc.update(|d| {
            d.add(note);
        })
        .await?;

        Ok(note_id)
    }

    /// Update a note's content.
    ///
    /// # Arguments
    ///
    /// * `note_id` - The note to update
    /// * `content` - New markdown content
    pub async fn update_note(&self, note_id: NoteId, content: impl Into<String>) -> Result<()> {
        let content = content.into();
        let doc = self.notes().await?;
        doc.update(|d| {
            if let Some(note) = d.find_mut(&note_id) {
                note.update_content(content);
            }
        })
        .await?;

        Ok(())
    }

    /// Delete a note.
    ///
    /// # Arguments
    ///
    /// * `note_id` - The note to delete
    pub async fn delete_note(&self, note_id: NoteId) -> Result<Option<Note>> {
        let mut removed = None;
        let doc = self.notes().await?;
        doc.update(|d| {
            removed = d.remove(&note_id);
        })
        .await?;

        Ok(removed)
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
    // Proof Folders
    // ============================================================

    /// Get the proof folders document for this realm.
    ///
    /// Proof folders contain multi-artifact documentation of quest fulfillment
    /// with a narrative explanation and supporting media.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let folders = realm.proof_folders().await?;
    /// let all_folders = folders.read().await.folders.clone();
    /// ```
    pub async fn proof_folders(&self) -> Result<Document<crate::proof_folder::ProofFolderDocument>> {
        self.document("proof_folders").await
    }

    /// Create a new proof folder in draft status.
    ///
    /// The folder starts empty and in draft status. Add a narrative and
    /// artifacts, then call `submit_proof_folder()` to finalize.
    ///
    /// # Arguments
    ///
    /// * `quest_id` - The quest this proof is for
    /// * `claimant` - The member creating the proof (typically your own ID)
    ///
    /// # Returns
    ///
    /// The ID of the new proof folder.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let folder_id = realm.create_proof_folder(quest_id, my_id).await?;
    /// realm.update_proof_folder_narrative(folder_id, "# My Work\n\nI did the thing...").await?;
    /// ```
    pub async fn create_proof_folder(
        &self,
        quest_id: QuestId,
        claimant: MemberId,
    ) -> Result<crate::proof_folder::ProofFolderId> {
        use crate::proof_folder::ProofFolder;

        let folder = ProofFolder::new(quest_id, claimant);
        let folder_id = folder.id;

        let doc = self.proof_folders().await?;
        doc.update(|d| {
            d.add(folder);
        })
        .await?;

        Ok(folder_id)
    }

    /// Update the narrative in a proof folder.
    ///
    /// The narrative is a markdown document explaining what work was done.
    /// Only works while folder is in draft status.
    ///
    /// # Arguments
    ///
    /// * `folder_id` - The proof folder to update
    /// * `narrative` - The new narrative content (markdown)
    ///
    /// # Example
    ///
    /// ```ignore
    /// realm.update_proof_folder_narrative(folder_id, "## Work completed\n\nI finished the task by...").await?;
    /// ```
    pub async fn update_proof_folder_narrative(
        &self,
        folder_id: crate::proof_folder::ProofFolderId,
        narrative: impl Into<String>,
    ) -> Result<()> {
        use crate::proof_folder::ProofFolderError;

        let narrative = narrative.into();
        let doc = self.proof_folders().await?;

        let mut result = Ok(());
        doc.update(|d| {
            if let Some(folder) = d.find_mut(&folder_id) {
                result = folder.set_narrative(&narrative).map_err(|e| match e {
                    ProofFolderError::NotDraft => IndraError::InvalidOperation(
                        "Cannot update narrative: folder is not in draft status".into(),
                    ),
                    _ => IndraError::InvalidOperation(e.to_string()),
                });
            } else {
                result = Err(IndraError::InvalidOperation("Proof folder not found".into()));
            }
        })
        .await?;

        result
    }

    /// Add an artifact to a proof folder.
    ///
    /// Only works while folder is in draft status. The artifact should
    /// already be stored via `share_artifact()`.
    ///
    /// # Arguments
    ///
    /// * `folder_id` - The proof folder to update
    /// * `artifact` - Metadata for the artifact to add
    ///
    /// # Example
    ///
    /// ```ignore
    /// let artifact = ProofFolderArtifact::new(photo_hash, "before.jpg", 1024, Some("image/jpeg".into()));
    /// realm.add_artifact_to_proof_folder(folder_id, artifact).await?;
    /// ```
    pub async fn add_artifact_to_proof_folder(
        &self,
        folder_id: crate::proof_folder::ProofFolderId,
        artifact: crate::proof_folder::ProofFolderArtifact,
    ) -> Result<()> {
        use crate::proof_folder::ProofFolderError;

        let doc = self.proof_folders().await?;

        let mut result = Ok(());
        doc.update(|d| {
            if let Some(folder) = d.find_mut(&folder_id) {
                result = folder.add_artifact(artifact.clone()).map_err(|e| match e {
                    ProofFolderError::NotDraft => IndraError::InvalidOperation(
                        "Cannot add artifact: folder is not in draft status".into(),
                    ),
                    _ => IndraError::InvalidOperation(e.to_string()),
                });
            } else {
                result = Err(IndraError::InvalidOperation("Proof folder not found".into()));
            }
        })
        .await?;

        result
    }

    /// Remove an artifact from a proof folder.
    ///
    /// Only works while folder is in draft status.
    ///
    /// # Arguments
    ///
    /// * `folder_id` - The proof folder to update
    /// * `artifact_id` - The artifact ID to remove
    pub async fn remove_artifact_from_proof_folder(
        &self,
        folder_id: crate::proof_folder::ProofFolderId,
        artifact_id: ArtifactId,
    ) -> Result<()> {
        use crate::proof_folder::ProofFolderError;

        let doc = self.proof_folders().await?;

        let mut result = Ok(());
        doc.update(|d| {
            if let Some(folder) = d.find_mut(&folder_id) {
                result = folder.remove_artifact(&artifact_id).map_err(|e| match e {
                    ProofFolderError::NotDraft => IndraError::InvalidOperation(
                        "Cannot remove artifact: folder is not in draft status".into(),
                    ),
                    ProofFolderError::ArtifactNotFound => {
                        IndraError::InvalidOperation("Artifact not found in folder".into())
                    }
                    _ => IndraError::InvalidOperation(e.to_string()),
                });
            } else {
                result = Err(IndraError::InvalidOperation("Proof folder not found".into()));
            }
        })
        .await?;

        result
    }

    /// Submit a proof folder for review.
    ///
    /// Changes the folder status to Submitted, creates/updates a QuestClaim
    /// linking to the folder, and posts a chat notification to the realm.
    ///
    /// This action is irreversible - once submitted, the folder cannot be edited.
    ///
    /// # Arguments
    ///
    /// * `folder_id` - The proof folder to submit
    ///
    /// # Returns
    ///
    /// The index of the claim in the quest's claims list.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Prepare folder
    /// let folder_id = realm.create_proof_folder(quest_id, my_id).await?;
    /// realm.update_proof_folder_narrative(folder_id, "Work done...").await?;
    /// // ... add artifacts ...
    ///
    /// // Submit for review (triggers chat notification)
    /// let claim_index = realm.submit_proof_folder(folder_id).await?;
    /// ```
    pub async fn submit_proof_folder(
        &self,
        folder_id: crate::proof_folder::ProofFolderId,
    ) -> Result<usize> {
        use crate::message::{Content, MessagePayload};
        use crate::proof_folder::ProofFolderError;

        // First, get folder info and submit it
        let doc = self.proof_folders().await?;
        let guard = doc.read().await;
        let folder = guard.find(&folder_id).ok_or_else(|| {
            IndraError::InvalidOperation("Proof folder not found".into())
        })?;

        if folder.is_submitted() {
            return Err(IndraError::InvalidOperation(
                "Proof folder has already been submitted".into(),
            ));
        }

        let quest_id = folder.quest_id;
        let claimant = folder.claimant;
        let narrative_preview = folder.narrative_preview();
        let artifact_count = folder.artifact_count();

        drop(guard);

        // Submit the folder
        let mut submit_result = Ok(());
        doc.update(|d| {
            if let Some(f) = d.find_mut(&folder_id) {
                submit_result = f.submit().map_err(|e| match e {
                    ProofFolderError::AlreadySubmitted => IndraError::InvalidOperation(
                        "Proof folder has already been submitted".into(),
                    ),
                    _ => IndraError::InvalidOperation(e.to_string()),
                });
            }
        })
        .await?;

        submit_result?;

        // Create or update quest claim with proof folder
        let mut claim_index = 0usize;
        let quests = self.quests().await?;
        quests
            .update(|d| {
                if let Some(quest) = d.find_mut(&quest_id) {
                    // Check if claimant already has a claim
                    if let Some((idx, claim)) = quest
                        .claims
                        .iter_mut()
                        .enumerate()
                        .find(|(_, c)| c.claimant == claimant)
                    {
                        // Update existing claim with proof folder
                        claim.set_proof_folder(folder_id);
                        claim_index = idx;
                    } else {
                        // Create new claim with proof folder
                        let claim = crate::quest::QuestClaim::with_proof_folder(claimant, folder_id);
                        quest.claims.push(claim);
                        claim_index = quest.claims.len() - 1;
                    }
                }
            })
            .await?;

        // Post chat notification
        let content = Content::ProofFolderSubmitted {
            quest_id,
            claimant,
            folder_id,
            narrative_preview,
            artifact_count,
        };

        let payload = MessagePayload::new(content);
        let bytes = postcard::to_allocvec(&payload)?;
        self.node.send_message(&self.id, bytes).await?;

        Ok(claim_index)
    }

    // ============================================================
    // Attention Tracking
    // ============================================================

    /// Get the attention tracking document for this realm.
    ///
    /// The attention document tracks which members are focused on which quests,
    /// enabling attention-based quest ranking.
    pub async fn attention(&self) -> Result<Document<AttentionDocument>> {
        self.document("attention").await
    }

    /// Focus on a specific quest.
    ///
    /// Members can focus on one quest at a time. Focusing on a new quest
    /// automatically ends focus on any previous quest. Time spent focusing
    /// contributes to the quest's attention ranking.
    ///
    /// # Arguments
    ///
    /// * `quest_id` - The quest to focus on
    /// * `member` - The member focusing (typically your own ID)
    ///
    /// # Returns
    ///
    /// The event ID of the attention switch event.
    pub async fn focus_on_quest(
        &self,
        quest_id: QuestId,
        member: MemberId,
    ) -> Result<AttentionEventId> {
        let mut event_id = [0u8; 16];
        let doc = self.attention().await?;
        doc.update(|d| {
            event_id = d.focus_on_quest(member, quest_id);
        })
        .await?;

        Ok(event_id)
    }

    /// Clear attention (stop focusing on any quest).
    ///
    /// Call this when you want to stop contributing attention to any quest.
    ///
    /// # Arguments
    ///
    /// * `member` - The member clearing attention (typically your own ID)
    ///
    /// # Returns
    ///
    /// The event ID of the attention clear event.
    pub async fn clear_attention(&self, member: MemberId) -> Result<AttentionEventId> {
        let mut event_id = [0u8; 16];
        let doc = self.attention().await?;
        doc.update(|d| {
            event_id = d.clear_attention(member);
        })
        .await?;

        Ok(event_id)
    }

    /// Get current focus for a member.
    ///
    /// Returns the quest ID the member is currently focused on, or None
    /// if they're not focused on any quest.
    pub async fn get_member_focus(&self, member: &MemberId) -> Result<Option<QuestId>> {
        let doc = self.attention().await?;
        Ok(doc.read().await.current_focus(member))
    }

    /// Get all members currently focusing on a quest.
    pub async fn get_quest_focusers(&self, quest_id: &QuestId) -> Result<Vec<MemberId>> {
        let doc = self.attention().await?;
        Ok(doc.read().await.members_focusing_on(quest_id))
    }

    /// Get quests ranked by total attention time.
    ///
    /// Returns quests sorted by accumulated attention (highest first).
    /// Use this for attention-based quest prioritization in the UI.
    pub async fn quests_by_attention(&self) -> Result<Vec<QuestAttention>> {
        let doc = self.attention().await?;
        Ok(doc.read().await.quests_by_attention(None))
    }

    /// Get attention details for a specific quest.
    pub async fn quest_attention(&self, quest_id: &QuestId) -> Result<QuestAttention> {
        let doc = self.attention().await?;
        Ok(doc.read().await.quest_attention(quest_id, None))
    }

    // ============================================================
    // Blessings
    // ============================================================

    /// Get the blessings document for this realm.
    ///
    /// The blessing document tracks which attention events have been
    /// released as validation for quest proofs.
    pub async fn blessings(&self) -> Result<Document<BlessingDocument>> {
        self.document("blessings").await
    }

    /// Bless a quest claim by releasing accumulated attention.
    ///
    /// Members who contributed attention to a quest can validate a proof
    /// by releasing their accumulated attention as a "blessing". This
    /// automatically posts a BlessingGiven message to the realm chat.
    ///
    /// # Arguments
    ///
    /// * `quest_id` - The quest being blessed
    /// * `claimant` - The member who submitted the proof
    /// * `blesser` - The member giving the blessing (typically your own ID)
    /// * `event_indices` - Indices into AttentionDocument.events to release
    ///
    /// # Returns
    ///
    /// The blessing ID if successful.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The event indices have already been blessed for this quest
    /// - The blesser doesn't own the specified attention events
    pub async fn bless_claim(
        &self,
        quest_id: QuestId,
        claimant: MemberId,
        blesser: MemberId,
        event_indices: Vec<usize>,
    ) -> Result<BlessingId> {
        // Validate that blesser owns the attention events
        let attention_doc = self.attention().await?;
        let attention = attention_doc.read().await;
        let events = attention.events();

        for &idx in &event_indices {
            if idx >= events.len() {
                return Err(IndraError::InvalidOperation(format!(
                    "Invalid event index: {} (only {} events exist)",
                    idx,
                    events.len()
                )));
            }
            let event = &events[idx];
            if event.member != blesser {
                return Err(IndraError::InvalidOperation(format!(
                    "Event {} belongs to different member, not blesser",
                    idx
                )));
            }
            // Validate that the event is for the correct quest
            if event.quest_id != Some(quest_id) {
                return Err(IndraError::InvalidOperation(format!(
                    "Event {} is for different quest",
                    idx
                )));
            }
        }
        drop(attention);

        // Record the blessing
        let claim_id = ClaimId::new(quest_id, claimant);
        let mut blessing_id = [0u8; 16];
        let blessing_doc = self.blessings().await?;

        let event_indices_clone = event_indices.clone();
        blessing_doc
            .update(|d| {
                match d.bless_claim(claim_id, blesser, event_indices_clone) {
                    Ok(id) => blessing_id = id,
                    Err(e) => {
                        // Log the error - we can't return it from the closure
                        tracing::warn!("Blessing failed: {}", e);
                    }
                }
            })
            .await?;

        // Post BlessingGiven message to chat
        self.send(Content::BlessingGiven {
            quest_id,
            claimant,
            blesser,
            event_indices,
        })
        .await?;

        Ok(blessing_id)
    }

    /// Get all blessings for a specific quest claim.
    pub async fn blessings_for_claim(
        &self,
        quest_id: QuestId,
        claimant: MemberId,
    ) -> Result<Vec<Blessing>> {
        let claim_id = ClaimId::new(quest_id, claimant);
        let doc = self.blessings().await?;
        Ok(doc
            .read()
            .await
            .blessings_for_claim(&claim_id)
            .into_iter()
            .cloned()
            .collect())
    }

    /// Get the total blessed attention duration for a quest claim.
    ///
    /// This calculates the duration by looking up the blessed event indices
    /// in the AttentionDocument and computing the time spans.
    pub async fn blessed_attention_duration(
        &self,
        quest_id: QuestId,
        claimant: MemberId,
    ) -> Result<Duration> {
        let claim_id = ClaimId::new(quest_id, claimant);
        let blessing_doc = self.blessings().await?;
        let attention_doc = self.attention().await?;

        let blessings_data = blessing_doc.read().await;
        let attention_data = attention_doc.read().await;
        let events = attention_data.events();

        let mut total_millis: u64 = 0;

        for blessing in blessings_data.blessings_for_claim(&claim_id) {
            // Calculate duration for each blessed event
            for &idx in &blessing.event_indices {
                if idx < events.len() {
                    let event = &events[idx];
                    // Find the next event from this member or use current time
                    let end_time = events
                        .iter()
                        .skip(idx + 1)
                        .find(|e| e.member == event.member)
                        .map(|e| e.timestamp_millis)
                        .unwrap_or_else(|| chrono::Utc::now().timestamp_millis());

                    let duration = (end_time - event.timestamp_millis).max(0) as u64;
                    total_millis += duration;
                }
            }
        }

        Ok(Duration::from_millis(total_millis))
    }

    /// Get attention event indices that haven't been blessed yet.
    ///
    /// Returns indices into AttentionDocument.events that belong to the
    /// specified member for the specified quest and haven't been used
    /// in any blessing yet.
    pub async fn unblessed_event_indices(
        &self,
        member: MemberId,
        quest_id: QuestId,
    ) -> Result<Vec<usize>> {
        let attention_doc = self.attention().await?;
        let blessing_doc = self.blessings().await?;

        let attention_data = attention_doc.read().await;
        let blessing_data = blessing_doc.read().await;
        let events = attention_data.events();

        // Find all event indices for this member on this quest
        let candidate_indices: Vec<usize> = events
            .iter()
            .enumerate()
            .filter(|(_, e)| e.member == member && e.quest_id == Some(quest_id))
            .map(|(idx, _)| idx)
            .collect();

        // Filter out already blessed indices
        Ok(blessing_data.unblessed_event_indices(&member, &quest_id, &candidate_indices))
    }

    /// Get the total unblessed attention duration available for blessing.
    pub async fn unblessed_attention_duration(
        &self,
        member: MemberId,
        quest_id: QuestId,
    ) -> Result<Duration> {
        let unblessed = self.unblessed_event_indices(member, quest_id).await?;
        let attention_doc = self.attention().await?;
        let attention_data = attention_doc.read().await;
        let events = attention_data.events();

        let mut total_millis: u64 = 0;

        for idx in unblessed {
            if idx < events.len() {
                let event = &events[idx];
                let end_time = events
                    .iter()
                    .skip(idx + 1)
                    .find(|e| e.member == event.member)
                    .map(|e| e.timestamp_millis)
                    .unwrap_or_else(|| chrono::Utc::now().timestamp_millis());

                let duration = (end_time - event.timestamp_millis).max(0) as u64;
                total_millis += duration;
            }
        }

        Ok(Duration::from_millis(total_millis))
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
            is_encrypted: false,
            sharing_status: crate::artifact_sharing::SharingStatus::Shared,
        };

        Ok(artifact)
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
    /// This is similar to `share_artifact()` but encrypts the content with
    /// a per-artifact key, enabling the artifact to be recalled later.
    ///
    /// # How It Works
    ///
    /// 1. Generate a random per-artifact encryption key
    /// 2. Encrypt the file content with the key
    /// 3. Store the encrypted content in blob storage
    /// 4. Store the encrypted key in the realm's key registry
    /// 5. Broadcast the artifact metadata to all members
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the file to share
    ///
    /// # Example
    ///
    /// ```ignore
    /// let artifact = realm.share_artifact_revocable("./secret.pdf").await?;
    /// println!("Shared revocable: {} (encrypted)", artifact.name);
    ///
    /// // Later, recall it
    /// realm.recall_artifact(&artifact.id).await?;
    /// ```
    pub async fn share_artifact_revocable(
        &self,
        path: impl AsRef<Path>,
    ) -> Result<crate::artifact_sharing::SharedArtifact> {
        use crate::artifact_sharing::{EncryptedArtifactKey, SharedArtifact, SharingStatus};

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

        // Guess MIME type from extension
        let mime_type = path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| guess_mime_type(ext));

        // Generate per-artifact encryption key
        let artifact_key = indras_crypto::generate_artifact_key();

        // Encrypt the content
        let encrypted = indras_crypto::encrypt_artifact(&file_data, &artifact_key)
            .map_err(|e| IndraError::Artifact(format!("Failed to encrypt artifact: {}", e)))?;

        // Get the encrypted bytes
        let encrypted_bytes = encrypted.to_bytes();

        // Compute BLAKE3 hash of ENCRYPTED content (this is the artifact ID)
        let hash = blake3::hash(&encrypted_bytes);
        let artifact_hash: [u8; 32] = *hash.as_bytes();

        // Get encrypted content size
        let size = encrypted_bytes.len() as u64;

        // Store encrypted content in blob storage
        let _content_ref = self
            .node
            .storage()
            .store_blob(&encrypted_bytes)
            .await
            .map_err(|e| IndraError::Artifact(format!("Failed to store encrypted blob: {}", e)))?;

        debug!(
            artifact_hash = %hex::encode(&artifact_hash[..8]),
            name = %name,
            encrypted_size = size,
            "Stored encrypted artifact in blob storage"
        );

        // Get our identity
        let sharer_id = self.node.identity();
        let sharer_hex = hex::encode(&sharer_id.as_bytes());

        // Get current tick (approximation - use current timestamp in millis)
        let shared_at = chrono::Utc::now().timestamp_millis() as u64;

        // Create the SharedArtifact
        let shared_artifact = SharedArtifact {
            hash: artifact_hash,
            name: name.clone(),
            size,
            mime_type: mime_type.clone(),
            sharer: sharer_hex.clone(),
            shared_at,
            status: SharingStatus::Shared,
        };

        // Encrypt the artifact key with the realm's interface key (placeholder)
        // In a full implementation, we'd get the interface key from the node
        // For now, we store a simple encrypted key structure
        let encrypted_key = EncryptedArtifactKey {
            nonce: [0u8; 12], // In real impl, generate random nonce
            ciphertext: artifact_key.to_vec(), // In real impl, encrypt with interface key
        };

        // Store in key registry
        let registry = self.artifact_key_registry().await?;
        registry
            .update(|r| {
                r.store(shared_artifact.clone(), encrypted_key);
            })
            .await?;

        // Post ArtifactRecalled-style notification to chat (as artifact shared)
        // This is so the chat shows when artifacts are shared
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
                is_encrypted: false,
                sharing_status: crate::artifact_sharing::SharingStatus::Shared,
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
