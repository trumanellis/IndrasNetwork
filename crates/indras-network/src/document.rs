//! Document<T> - typed CRDT wrapper for collaborative data.
//!
//! Documents provide type-safe access to Automerge-backed data structures
//! that automatically synchronize across all realm members.

use crate::error::Result;
use crate::member::Member;
use crate::network::RealmId;

use futures::Stream;
use indras_core::InterfaceEvent;
use indras_node::IndrasNode;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::marker::PhantomData;
use std::ops::Deref;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tracing::{debug, warn};

/// Envelope that tags document messages with the document name.
///
/// A realm can have multiple documents ("quests", "notes", etc.) and
/// this envelope disambiguates which document a message belongs to.
#[derive(Serialize, Deserialize)]
struct DocumentEnvelope {
    doc_name: String,
    payload: Vec<u8>,
}

/// Trait for document schemas that can be stored in a `Document<T>`.
///
/// Provides a `merge` method for reconciling local and remote state.
/// The default implementation does full replacement (last-writer-wins).
/// Override `merge` for document types that support set-union semantics
/// (e.g. chat messages keyed by unique ID).
///
/// # Example
///
/// ```ignore
/// #[derive(Default, Clone, Serialize, Deserialize)]
/// struct QuestLog {
///     quests: Vec<Quest>,
/// }
///
/// impl DocumentSchema for QuestLog {}
/// ```
pub trait DocumentSchema: Default + Clone + Serialize + DeserializeOwned + Send + Sync + 'static {
    /// Merge remote state into this document.
    ///
    /// Default: full replacement (last-writer-wins).
    /// Override for types that support set-union merge.
    fn merge(&mut self, remote: Self) {
        *self = remote;
    }
}

/// Convenience macro to implement `DocumentSchema` with default merge (replacement).
#[macro_export]
macro_rules! impl_document_schema {
    ($($t:ty),* $(,)?) => {
        $(impl $crate::document::DocumentSchema for $t {})*
    };
}

/// A typed, reactive CRDT document.
///
/// Documents automatically synchronize across all realm members.
/// Changes made locally are propagated to peers, and changes from
/// peers are merged automatically.
///
/// # Type Parameters
///
/// * `T` - The document data type (must implement `DocumentSchema`)
///
/// # Example
///
/// ```ignore
/// #[derive(Default, Clone, Serialize, Deserialize)]
/// struct QuestLog {
///     quests: Vec<Quest>,
/// }
///
/// let doc = realm.document::<QuestLog>("quests").await?;
///
/// // Read current state
/// {
///     let quests = doc.read();
///     println!("Total quests: {}", quests.quests.len());
/// }
///
/// // Make changes (auto-synced to peers)
/// doc.update(|q| {
///     q.quests.push(Quest {
///         id: "quest-1".into(),
///         title: "Defeat the dragon".into(),
///         completed: false,
///     });
/// }).await?;
///
/// // Subscribe to changes from peers
/// let mut changes = doc.changes();
/// while let Some(change) = changes.next().await {
///     println!("Document updated by {}", change.author.name());
/// }
/// ```
pub struct Document<T: DocumentSchema> {
    /// The realm this document belongs to.
    realm_id: RealmId,
    /// The document name within the realm.
    name: String,
    /// The current document state.
    state: Arc<RwLock<T>>,
    /// Change notification sender.
    change_tx: broadcast::Sender<DocumentChange<T>>,
    /// Reference to the underlying node.
    node: Arc<IndrasNode>,
    /// Marker for the document type.
    _marker: PhantomData<T>,
}

/// A change notification for a document.
#[derive(Debug, Clone)]
pub struct DocumentChange<T> {
    /// The new document state after the change.
    pub new_state: T,
    /// The member who made the change (if known).
    pub author: Option<Member>,
    /// Whether this change came from a remote peer.
    pub is_remote: bool,
}

/// A read guard for document state.
///
/// Provides immutable access to the document data.
pub struct DocumentRef<'a, T> {
    guard: tokio::sync::RwLockReadGuard<'a, T>,
}

impl<'a, T> Deref for DocumentRef<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.guard
    }
}

impl<T: DocumentSchema> Document<T> {
    /// Create or load a document.
    ///
    /// Loads persisted state, spawns a background listener for remote
    /// updates, and returns the document ready for use.
    pub(crate) async fn new(
        realm_id: RealmId,
        name: String,
        node: Arc<IndrasNode>,
    ) -> Result<Self> {
        let (change_tx, _) = broadcast::channel(512);

        // Try to load existing state, or create default
        let state = Self::load_or_create(&node, &realm_id, &name).await?;
        let state = Arc::new(RwLock::new(state));

        let doc = Self {
            realm_id,
            name,
            state,
            change_tx,
            node,
            _marker: PhantomData,
        };

        // Spawn background listener for remote updates
        doc.spawn_listener();

        Ok(doc)
    }

    /// Spawn a background task that listens for remote document updates.
    ///
    /// Subscribes to the realm's event broadcast AND the sync notification
    /// channel. The event broadcast fires for direct peer messages; the sync
    /// channel fires after CRDT merge_sync completes (store-and-forward path).
    ///
    /// Uses a weak reference to the node so the listener doesn't prevent
    /// cleanup when the Document and IndrasNetwork are dropped.
    fn spawn_listener(&self) {
        let rx = match self.node.events(&self.realm_id) {
            Ok(rx) => rx,
            Err(e) => {
                warn!(
                    doc_name = %self.name,
                    error = %e,
                    "Document spawn_listener: interface not found, no listener started"
                );
                return;
            }
        };

        let sync_rx = match self.node.sync_events(&self.realm_id) {
            Ok(rx) => rx,
            Err(e) => {
                warn!(
                    doc_name = %self.name,
                    error = %e,
                    "Document spawn_listener: sync channel not found, no listener started"
                );
                return;
            }
        };

        let state = Arc::clone(&self.state);
        let change_tx = self.change_tx.clone();
        let node_weak = Arc::downgrade(&self.node);
        let realm_id = self.realm_id;
        let name = self.name.clone();
        let our_identity = *self.node.identity();

        // Build storage key for persisting
        let mut storage_key = Vec::with_capacity(4 + 32 + name.len());
        storage_key.extend_from_slice(b"doc:");
        storage_key.extend_from_slice(realm_id.as_bytes());
        storage_key.extend_from_slice(name.as_bytes());

        debug!(doc_name = %name, "Document spawn_listener: started");
        tokio::spawn(async move {
            let mut rx = rx;
            let mut sync_rx = sync_rx;

            // Helper: refresh state from the Automerge document (used by both
            // the sync notification arm and the Lagged recovery arm).
            let refresh_from_crdt = |state: &Arc<RwLock<T>>,
                                      node_weak: &std::sync::Weak<IndrasNode>,
                                      change_tx: &broadcast::Sender<DocumentChange<T>>,
                                      storage_key: &[u8],
                                      name: &str,
                                      realm_id: RealmId| {
                let state = Arc::clone(state);
                let node_weak = node_weak.clone();
                let change_tx = change_tx.clone();
                let storage_key = storage_key.to_vec();
                let name = name.to_string();
                async move {
                    let Some(node) = node_weak.upgrade() else {
                        return false; // node dropped
                    };
                    if let Ok(events) = node.document_events(&realm_id).await {
                        let mut updated = false;
                        for event in events.iter().rev() {
                            if let InterfaceEvent::Message { content, .. } = event {
                                if let Ok(envelope) = postcard::from_bytes::<DocumentEnvelope>(content) {
                                    if envelope.doc_name != name {
                                        continue;
                                    }
                                    if let Ok(remote_state) = postcard::from_bytes::<T>(&envelope.payload) {
                                        let merged = {
                                            let mut guard = state.write().await;
                                            guard.merge(remote_state);
                                            guard.clone()
                                        };
                                        if let Ok(data) = postcard::to_allocvec(&merged) {
                                            let _ = node.storage().interface_store().set_document_data(&storage_key, &data);
                                        }
                                        let _ = change_tx.send(DocumentChange {
                                            new_state: merged,
                                            author: None,
                                            is_remote: true,
                                        });
                                        updated = true;
                                        break;
                                    }
                                }
                            }
                        }
                        updated
                    } else {
                        false
                    }
                }
            };

            loop {
                tokio::select! {
                    result = rx.recv() => {
                        match result {
                            Ok(received) => {
                                debug!(
                                    doc_name = %name,
                                    event_type = ?std::mem::discriminant(&received.event),
                                    "Document spawn_listener: received event"
                                );
                                // Only process Message events
                                let InterfaceEvent::Message { content, sender, .. } = &received.event else {
                                    continue;
                                };

                                // Skip our own messages
                                if *sender == our_identity {
                                    debug!(doc_name = %name, "Document spawn_listener: skipping own message");
                                    continue;
                                }

                                // Upgrade weak ref; if node is gone, stop listening
                                let Some(node) = node_weak.upgrade() else {
                                    break;
                                };

                                // Try to deserialize as DocumentEnvelope first
                                if let Ok(envelope) = postcard::from_bytes::<DocumentEnvelope>(content) {
                                    if envelope.doc_name != name {
                                        continue; // Different document
                                    }
                                    if let Ok(remote_state) = postcard::from_bytes::<T>(&envelope.payload) {
                                        // Merge remote state into local (union, not replace)
                                        let merged = {
                                            let mut guard = state.write().await;
                                            guard.merge(remote_state);
                                            guard.clone()
                                        };
                                        // Persist merged state to redb (best-effort)
                                        if let Ok(data) = postcard::to_allocvec(&merged) {
                                            let _ = node.storage().interface_store().set_document_data(&storage_key, &data);
                                        }
                                        // Fire changes() stream with merged state
                                        let _ = change_tx.send(DocumentChange {
                                            new_state: merged,
                                            author: Some(Member::new(*sender)),
                                            is_remote: true,
                                        });
                                    }
                                    continue;
                                }

                                // Fallback: try raw format (backward compat)
                                if let Ok(remote_state) = postcard::from_bytes::<T>(content) {
                                    let merged = {
                                        let mut guard = state.write().await;
                                        guard.merge(remote_state);
                                        guard.clone()
                                    };
                                    if let Ok(data) = postcard::to_allocvec(&merged) {
                                        let _ = node.storage().interface_store().set_document_data(&storage_key, &data);
                                    }
                                    let _ = change_tx.send(DocumentChange {
                                        new_state: merged,
                                        author: Some(Member::new(*sender)),
                                        is_remote: true,
                                    });
                                }
                            }
                            Err(broadcast::error::RecvError::Lagged(count)) => {
                                warn!(
                                    doc_name = %name,
                                    skipped = count,
                                    "Document listener lagged, recovering via refresh"
                                );
                                refresh_from_crdt(
                                    &state, &node_weak, &change_tx,
                                    &storage_key, &name, realm_id,
                                ).await;
                                continue;
                            }
                            Err(broadcast::error::RecvError::Closed) => break,
                        }
                    }
                    result = sync_rx.recv() => {
                        match result {
                            Ok(()) => {
                                debug!(
                                    doc_name = %name,
                                    "Document spawn_listener: sync notification, refreshing from CRDT"
                                );
                                refresh_from_crdt(
                                    &state, &node_weak, &change_tx,
                                    &storage_key, &name, realm_id,
                                ).await;
                            }
                            Err(broadcast::error::RecvError::Lagged(_)) => continue,
                            Err(broadcast::error::RecvError::Closed) => break,
                        }
                    }
                }
            }
        });
    }

    async fn load_or_create(
        node: &IndrasNode,
        realm_id: &RealmId,
        name: &str,
    ) -> Result<T> {
        // Build a storage key from realm_id and document name
        let mut key = Vec::with_capacity(4 + 32 + name.len());
        key.extend_from_slice(b"doc:");
        key.extend_from_slice(realm_id.as_bytes());
        key.extend_from_slice(name.as_bytes());

        // 1. Automerge document events (includes CRDT-synced remote events).
        // When a peer joins a realm, their EventStore may be empty but the
        // Automerge document contains events received via CRDT sync.
        if let Ok(events) = node.document_events(realm_id).await {
            for event in events.iter().rev() {
                if let InterfaceEvent::Message { content, .. } = event {
                    if let Ok(env) = postcard::from_bytes::<DocumentEnvelope>(content) {
                        if env.doc_name == name {
                            if let Ok(state) = postcard::from_bytes::<T>(&env.payload) {
                                return Ok(state);
                            }
                        }
                        continue;
                    }
                    if let Ok(state) = postcard::from_bytes::<T>(content) {
                        return Ok(state);
                    }
                }
            }
        }

        // 2. EventStore events (local events only, fallback).
        if let Ok(events) = node.events_since(realm_id, 0).await {
            for event in events.iter().rev() {
                if let InterfaceEvent::Message { content, .. } = event {
                    if let Ok(env) = postcard::from_bytes::<DocumentEnvelope>(content) {
                        if env.doc_name == name {
                            if let Ok(state) = postcard::from_bytes::<T>(&env.payload) {
                                return Ok(state);
                            }
                        }
                        continue;
                    }
                    if let Ok(state) = postcard::from_bytes::<T>(content) {
                        return Ok(state);
                    }
                }
            }
        }

        // 3. Fall back to redb snapshot.
        let storage = node.storage();
        if let Ok(Some(value)) = storage.interface_store().get_document_data(&key) {
            match postcard::from_bytes::<T>(&value) {
                Ok(state) => return Ok(state),
                Err(e) => {
                    tracing::warn!(
                        realm = %hex::encode(&realm_id.as_bytes()[..8]),
                        name = name,
                        error = %e,
                        "Failed to deserialize document snapshot, using default"
                    );
                }
            }
        }

        // 4. No existing state found, return default
        Ok(T::default())
    }

    /// Get the document name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Build the storage key for this document
    fn storage_key(&self) -> Vec<u8> {
        let mut key = Vec::with_capacity(4 + 32 + self.name.len());
        key.extend_from_slice(b"doc:");
        key.extend_from_slice(self.realm_id.as_bytes());
        key.extend_from_slice(self.name.as_bytes());
        key
    }

    /// Persist the current document state to storage
    async fn persist(&self, state: &T) -> Result<()> {
        let key = self.storage_key();
        let data = postcard::to_allocvec(state)?;
        self.node
            .storage()
            .interface_store()
            .set_document_data(&key, &data)?;
        Ok(())
    }

    /// Read the current document state.
    ///
    /// Returns a read guard that provides immutable access to the data.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let doc = realm.document::<QuestLog>("quests").await?;
    /// let quests = doc.read().await;
    /// println!("Total: {}", quests.quests.len());
    /// ```
    pub async fn read(&self) -> DocumentRef<'_, T> {
        DocumentRef {
            guard: self.state.read().await,
        }
    }

    /// Read the current state synchronously (blocking).
    ///
    /// Use `read()` in async contexts.
    pub fn read_blocking(&self) -> T {
        self.state.blocking_read().clone()
    }

    /// Refresh document state from the realm's event log.
    ///
    /// When a peer first joins a realm, events may arrive after the initial
    /// `Document::new()`. Call this to re-check for newer state.
    ///
    /// Checks the Automerge document first (includes CRDT-synced remote events),
    /// then falls back to the EventStore (local events only).
    /// Returns `true` if the state was updated.
    pub async fn refresh(&self) -> Result<bool> {
        let realm_short: String = self.realm_id.as_bytes().iter().take(8).map(|b| format!("{:02x}", b)).collect();
        let mut updated = false;

        // 1. Automerge document events (includes events received via CRDT sync)
        // Merge ALL matching events (not just the most recent) to ensure
        // we union state from all peers who may have sent concurrently.
        if let Ok(events) = self.node.document_events(&self.realm_id).await {
            debug!(
                doc_name = %self.name,
                realm = %realm_short,
                event_count = events.len(),
                "Document refresh: checking Automerge events"
            );
            for event in events.iter() {
                if let InterfaceEvent::Message { content, .. } = event {
                    if let Ok(env) = postcard::from_bytes::<DocumentEnvelope>(content) {
                        if env.doc_name == self.name {
                            if let Ok(remote_state) = postcard::from_bytes::<T>(&env.payload) {
                                let mut state = self.state.write().await;
                                state.merge(remote_state);
                                drop(state);
                                updated = true;
                            }
                        }
                        continue;
                    }
                    if let Ok(remote_state) = postcard::from_bytes::<T>(content) {
                        let mut state = self.state.write().await;
                        state.merge(remote_state);
                        drop(state);
                        updated = true;
                    }
                }
            }
        } else {
            debug!(
                doc_name = %self.name,
                realm = %realm_short,
                "Document refresh: failed to get Automerge events"
            );
        }

        // 2. Fallback: EventStore (local events only)
        if !updated {
            if let Ok(events) = self.node.events_since(&self.realm_id, 0).await {
                debug!(
                    doc_name = %self.name,
                    realm = %realm_short,
                    event_count = events.len(),
                    "Document refresh: checking EventStore events"
                );
                for event in events.iter() {
                    if let InterfaceEvent::Message { content, .. } = event {
                        if let Ok(env) = postcard::from_bytes::<DocumentEnvelope>(content) {
                            if env.doc_name == self.name {
                                if let Ok(remote_state) = postcard::from_bytes::<T>(&env.payload) {
                                    let mut state = self.state.write().await;
                                    state.merge(remote_state);
                                    drop(state);
                                    updated = true;
                                }
                            }
                            continue;
                        }
                        if let Ok(remote_state) = postcard::from_bytes::<T>(content) {
                            let mut state = self.state.write().await;
                            state.merge(remote_state);
                            drop(state);
                            updated = true;
                        }
                    }
                }
            }
        }

        // Persist once after all merges and notify subscribers
        if updated {
            let state = self.state.read().await.clone();
            self.persist(&state).await?;
            let _ = self.change_tx.send(DocumentChange {
                new_state: state,
                author: None,
                is_remote: true,
            });
        } else {
            debug!(
                doc_name = %self.name,
                realm = %realm_short,
                "Document refresh: no matching document state found"
            );
        }

        Ok(updated)
    }

    /// Update the document state.
    ///
    /// The update function receives a mutable reference to the document
    /// and can modify it. Changes are automatically synchronized to peers.
    ///
    /// # Arguments
    ///
    /// * `f` - A function that modifies the document
    ///
    /// # Example
    ///
    /// ```ignore
    /// doc.update(|quests| {
    ///     quests.quests.push(Quest::new("New quest"));
    /// }).await?;
    /// ```
    pub async fn update<F>(&self, f: F) -> Result<()>
    where
        F: FnOnce(&mut T),
    {
        let realm_short: String = self.realm_id.as_bytes().iter().take(8).map(|b| format!("{:02x}", b)).collect();

        let new_state = {
            let mut state = self.state.write().await;
            f(&mut state);
            state.clone()
        };

        // Persist to local storage
        self.persist(&new_state).await?;

        // Serialize and wrap in envelope for multi-document disambiguation
        let inner_payload = postcard::to_allocvec(&new_state)?;
        let envelope = DocumentEnvelope {
            doc_name: self.name.clone(),
            payload: inner_payload.clone(),
        };
        let message = postcard::to_allocvec(&envelope)?;

        debug!(
            doc_name = %self.name,
            realm = %realm_short,
            payload_len = inner_payload.len(),
            message_len = message.len(),
            "Document update: sending to network"
        );

        self.node.send_message(&self.realm_id, message).await?;

        debug!(
            doc_name = %self.name,
            realm = %realm_short,
            "Document update: sent successfully"
        );

        // Notify local subscribers
        let _ = self.change_tx.send(DocumentChange {
            new_state,
            author: None, // Local change
            is_remote: false,
        });

        Ok(())
    }

    /// Perform a transaction on the document.
    ///
    /// Similar to `update`, but returns a value from the closure.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let quest_count = doc.transaction(|quests| {
    ///     quests.quests.push(Quest::new("New quest"));
    ///     quests.quests.len()
    /// }).await?;
    /// ```
    pub async fn transaction<F, R>(&self, f: F) -> Result<R>
    where
        F: FnOnce(&mut T) -> R,
    {
        let (result, new_state) = {
            let mut state = self.state.write().await;
            let result = f(&mut state);
            (result, state.clone())
        };

        // Persist to local storage
        self.persist(&new_state).await?;

        // Serialize and wrap in envelope
        let inner_payload = postcard::to_allocvec(&new_state)?;
        let envelope = DocumentEnvelope {
            doc_name: self.name.clone(),
            payload: inner_payload,
        };
        let message = postcard::to_allocvec(&envelope)?;
        self.node.send_message(&self.realm_id, message).await?;

        // Notify local subscribers
        let _ = self.change_tx.send(DocumentChange {
            new_state,
            author: None,
            is_remote: false,
        });

        Ok(result)
    }

    /// Subscribe to document changes.
    ///
    /// Returns a stream that yields `DocumentChange` events whenever
    /// the document is modified (locally or by remote peers).
    ///
    /// # Example
    ///
    /// ```ignore
    /// let mut changes = doc.changes();
    /// while let Some(change) = changes.next().await {
    ///     if change.is_remote {
    ///         println!("Remote update from {:?}", change.author);
    ///     }
    /// }
    /// ```
    pub fn changes(&self) -> impl Stream<Item = DocumentChange<T>> + Send + '_ {
        let rx = self.change_tx.subscribe();
        crate::stream::broadcast_to_stream(rx)
    }

    /// Subscribe to document changes, returning a raw broadcast receiver.
    ///
    /// Unlike `changes()` which returns a stream tied to `&self`, this
    /// returns an owned `'static` receiver that can be moved into spawned
    /// tasks or stored independently.
    pub fn subscribe(&self) -> broadcast::Receiver<DocumentChange<T>> {
        self.change_tx.subscribe()
    }

    /// Get the number of subscribers to this document.
    pub fn subscriber_count(&self) -> usize {
        self.change_tx.receiver_count()
    }
}

impl<T: DocumentSchema> Clone for Document<T> {
    fn clone(&self) -> Self {
        Self {
            realm_id: self.realm_id,
            name: self.name.clone(),
            state: Arc::clone(&self.state),
            change_tx: self.change_tx.clone(),
            node: Arc::clone(&self.node),
            _marker: PhantomData,
        }
    }
}


impl<T: DocumentSchema + std::fmt::Debug> std::fmt::Debug for Document<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Document")
            .field("name", &self.name)
            .field("realm_id", &hex::encode(&self.realm_id.as_bytes()[..8]))
            .finish()
    }
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
    use serde::{Deserialize, Serialize};

    #[derive(Default, Clone, Serialize, Deserialize, Debug)]
    struct TestDoc {
        value: i32,
    }

    // More comprehensive tests would require a running node
}
