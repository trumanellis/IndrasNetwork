//! Document<T> - typed CRDT wrapper for collaborative data.
//!
//! Documents provide type-safe access to Automerge-backed data structures
//! that automatically synchronize across all realm members.

use crate::error::Result;
use crate::member::Member;
use crate::network::RealmId;

use futures::Stream;
use indras_node::IndrasNode;
use serde::{de::DeserializeOwned, Serialize};
use std::marker::PhantomData;
use std::ops::Deref;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};

/// Marker trait for document schemas.
///
/// Types that implement this trait can be used as document data.
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
pub trait DocumentSchema: Default + Clone + Serialize + DeserializeOwned + Send + Sync + 'static {}

// Blanket implementation for any compatible type
impl<T> DocumentSchema for T where T: Default + Clone + Serialize + DeserializeOwned + Send + Sync + 'static {}

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
    pub(crate) async fn new(
        realm_id: RealmId,
        name: String,
        node: Arc<IndrasNode>,
    ) -> Result<Self> {
        let (change_tx, _) = broadcast::channel(64);

        // Try to load existing state, or create default
        let state = Self::load_or_create(&node, &realm_id, &name).await?;

        Ok(Self {
            realm_id,
            name,
            state: Arc::new(RwLock::new(state)),
            change_tx,
            node,
            _marker: PhantomData,
        })
    }

    async fn load_or_create(
        node: &IndrasNode,
        realm_id: &RealmId,
        name: &str,
    ) -> Result<T> {
        // Build a storage key from realm_id and document name
        // Format: "doc:" || realm_id (32 bytes) || name
        let mut key = Vec::with_capacity(4 + 32 + name.len());
        key.extend_from_slice(b"doc:");
        key.extend_from_slice(realm_id.as_bytes());
        key.extend_from_slice(name.as_bytes());

        // Try to load from the interface store's underlying redb storage
        // We use the SNAPSHOTS table with our document key prefix
        let storage = node.storage();

        // Access the redb storage through the interface store
        // The interface_store gives us access to the underlying RedbStorage
        if let Ok(Some(value)) = storage.interface_store().get_document_data(&key) {
            // Found existing document state - deserialize it
            match postcard::from_bytes::<T>(&value) {
                Ok(state) => return Ok(state),
                Err(e) => {
                    // Log deserialization error but fall back to default
                    tracing::warn!(
                        realm = %hex::encode(&realm_id.as_bytes()[..8]),
                        name = name,
                        error = %e,
                        "Failed to deserialize document state, using default"
                    );
                }
            }
        }

        // No existing state found, return default
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
        let new_state = {
            let mut state = self.state.write().await;
            f(&mut state);
            state.clone()
        };

        // Persist to local storage
        self.persist(&new_state).await?;

        // Serialize and send as event
        let payload = postcard::to_allocvec(&new_state)?;
        self.node.send_message(&self.realm_id, payload).await?;

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

        // Serialize and send as event
        let payload = postcard::to_allocvec(&new_state)?;
        self.node.send_message(&self.realm_id, payload).await?;

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
