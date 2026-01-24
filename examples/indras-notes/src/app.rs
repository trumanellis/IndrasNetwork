//! Application state and logic
//!
//! Manages the note-taking application state and operations.
#![allow(dead_code)] // Example code with reserved features

use thiserror::Error;
use tracing::warn;

use indras_core::{InterfaceId, PeerIdentity};
use indras_node::{IndrasNode, InviteKey, NodeConfig, NodeError};

use crate::note::{Note, NoteId, NoteOperation};
use crate::notebook::Notebook;
use crate::storage::{LocalStorage, StorageError, UserProfile};

/// Application errors
#[derive(Debug, Error)]
pub enum AppError {
    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),
    #[error("Node error: {0}")]
    Node(#[from] NodeError),
    #[error("Not initialized. Run 'init' first.")]
    NotInitialized,
    #[error("Notebook not found: {0}")]
    NotebookNotFound(String),
    #[error("Note not found: {0}")]
    NoteNotFound(String),
    #[error("Already initialized")]
    AlreadyInitialized,
    #[error("Lua error: {0}")]
    Lua(String),
    #[error("Serialization error: {0}")]
    Serialization(String),
}

/// The main application state
pub struct App {
    /// Local storage
    storage: LocalStorage,
    /// Network node (optional, created after init)
    node: Option<IndrasNode>,
    /// User profile
    profile: Option<UserProfile>,
    /// Currently open notebook
    current_notebook: Option<Notebook>,
}

impl App {
    /// Create a new application instance
    pub async fn new() -> Result<Self, AppError> {
        let storage = LocalStorage::default_location().await?;

        Ok(Self {
            storage,
            node: None,
            profile: None,
            current_notebook: None,
        })
    }

    /// Create with custom storage location
    pub async fn with_storage(storage: LocalStorage) -> Result<Self, AppError> {
        Ok(Self {
            storage,
            node: None,
            profile: None,
            current_notebook: None,
        })
    }

    /// Check if the app is initialized (has profile)
    pub async fn is_initialized(&self) -> bool {
        self.storage.has_profile().await
    }

    /// Initialize the app with a new identity
    pub async fn init(&mut self, name: &str) -> Result<(), AppError> {
        if self.storage.has_profile().await {
            return Err(AppError::AlreadyInitialized);
        }

        // Create node (generates identity)
        let node_config = NodeConfig::with_data_dir(self.storage.data_dir().join("node"));
        let node = IndrasNode::new(node_config).await?;

        // Save profile
        let identity = node.identity();
        let profile = UserProfile::new(
            name,
            identity.as_bytes(),
            vec![], // We don't store secret key in this demo
        );
        self.storage.save_profile(&profile).await?;

        self.profile = Some(profile);
        self.node = Some(node);

        Ok(())
    }

    /// Load existing profile and start node
    pub async fn load(&mut self) -> Result<(), AppError> {
        if !self.storage.has_profile().await {
            return Err(AppError::NotInitialized);
        }

        let profile = self.storage.load_profile().await?;

        // Create node
        let node_config = NodeConfig::with_data_dir(self.storage.data_dir().join("node"));
        let node = IndrasNode::new(node_config).await?;

        self.profile = Some(profile);
        self.node = Some(node);

        Ok(())
    }

    /// Get the user's name
    pub fn user_name(&self) -> Option<&str> {
        self.profile.as_ref().map(|p| p.name.as_str())
    }

    /// Get the user's short identity
    pub fn user_short_id(&self) -> Option<String> {
        self.node.as_ref().map(|n| n.identity().short_id())
    }

    /// Create a new notebook
    pub async fn create_notebook(&mut self, name: &str) -> Result<InterfaceId, AppError> {
        let node = self.node.as_ref().ok_or(AppError::NotInitialized)?;

        // Create interface via node
        let (interface_id, _invite) = node.create_interface(Some(name)).await?;

        // Create local notebook
        let notebook = Notebook::new(name, interface_id);
        self.storage.save_notebook(&notebook).await?;

        Ok(interface_id)
    }

    /// Join an existing notebook via invite
    pub async fn join_notebook(&mut self, invite_b64: &str) -> Result<InterfaceId, AppError> {
        let node = self.node.as_ref().ok_or(AppError::NotInitialized)?;

        // Parse invite
        let invite = InviteKey::from_base64(invite_b64)?;
        let interface_id = invite.interface_id;

        // Join via node
        node.join_interface(invite).await?;

        // Create local notebook (will sync name from peers)
        let notebook = Notebook::new("Shared Notebook", interface_id);
        self.storage.save_notebook(&notebook).await?;

        Ok(interface_id)
    }

    /// Open a notebook
    pub async fn open_notebook(&mut self, interface_id: &InterfaceId) -> Result<(), AppError> {
        let notebook = self.storage.load_notebook(interface_id).await?;
        self.current_notebook = Some(notebook);
        Ok(())
    }

    /// Get the current notebook
    pub fn current_notebook(&self) -> Option<&Notebook> {
        self.current_notebook.as_ref()
    }

    /// Get mutable current notebook
    pub fn current_notebook_mut(&mut self) -> Option<&mut Notebook> {
        self.current_notebook.as_mut()
    }

    /// List all notebooks
    pub async fn list_notebooks(&self) -> Result<Vec<crate::storage::NotebookMeta>, AppError> {
        Ok(self.storage.list_notebooks().await?)
    }

    /// Create a new note in the current notebook
    pub async fn create_note(&mut self, title: &str) -> Result<NoteId, AppError> {
        if self.current_notebook.is_none() {
            return Err(AppError::NotebookNotFound("No notebook open".to_string()));
        }

        let author = self
            .user_short_id()
            .unwrap_or_else(|| "unknown".to_string());

        let note = Note::new(title, author);
        let id = note.id.clone();

        // Now we can safely borrow mutably
        let notebook = self.current_notebook.as_mut().unwrap();
        let operation = NoteOperation::create(note);
        notebook.apply(operation.clone());
        self.save_current_notebook().await?;

        // Broadcast operation to other peers
        self.broadcast_operation(&operation).await;

        Ok(id)
    }

    /// Update a note's content
    pub async fn update_note_content(
        &mut self,
        id: &NoteId,
        content: &str,
    ) -> Result<(), AppError> {
        let notebook = self
            .current_notebook
            .as_mut()
            .ok_or_else(|| AppError::NotebookNotFound("No notebook open".to_string()))?;

        if notebook.get(id).is_none() {
            return Err(AppError::NoteNotFound(id.clone()));
        }

        let operation = NoteOperation::update_content(id, content);
        notebook.apply(operation.clone());
        self.save_current_notebook().await?;

        // Broadcast operation to other peers
        self.broadcast_operation(&operation).await;

        Ok(())
    }

    /// Update a note's title
    pub async fn update_note_title(&mut self, id: &NoteId, title: &str) -> Result<(), AppError> {
        let notebook = self
            .current_notebook
            .as_mut()
            .ok_or_else(|| AppError::NotebookNotFound("No notebook open".to_string()))?;

        if notebook.get(id).is_none() {
            return Err(AppError::NoteNotFound(id.clone()));
        }

        let operation = NoteOperation::update_title(id, title);
        notebook.apply(operation.clone());
        self.save_current_notebook().await?;

        // Broadcast operation to other peers
        self.broadcast_operation(&operation).await;

        Ok(())
    }

    /// Delete a note
    pub async fn delete_note(&mut self, id: &NoteId) -> Result<(), AppError> {
        let notebook = self
            .current_notebook
            .as_mut()
            .ok_or_else(|| AppError::NotebookNotFound("No notebook open".to_string()))?;

        let operation = NoteOperation::delete(id);
        notebook.apply(operation.clone());
        self.save_current_notebook().await?;

        // Broadcast operation to other peers
        self.broadcast_operation(&operation).await;

        Ok(())
    }

    /// Get a note by ID (supports partial ID matching)
    pub fn find_note(&self, partial_id: &str) -> Option<&Note> {
        let notebook = self.current_notebook.as_ref()?;

        // Try exact match first
        if let Some(note) = notebook.get(&partial_id.to_string()) {
            return Some(note);
        }

        // Try prefix match
        notebook
            .notes
            .values()
            .find(|n| n.id.starts_with(partial_id))
    }

    /// Get invite key for current notebook
    pub fn get_invite(&self) -> Result<String, AppError> {
        let notebook = self
            .current_notebook
            .as_ref()
            .ok_or_else(|| AppError::NotebookNotFound("No notebook open".to_string()))?;

        let invite = InviteKey::new(notebook.interface_id);
        invite
            .to_base64()
            .map_err(|e| AppError::Node(NodeError::Serialization(e.to_string())))
    }

    /// Broadcast a note operation to other peers
    ///
    /// This method will log a warning but not fail if broadcasting fails,
    /// since the operation has already been applied locally.
    async fn broadcast_operation(&self, operation: &NoteOperation) {
        // Get the node and current notebook
        let Some(node) = &self.node else {
            warn!("Cannot broadcast operation: node not initialized");
            return;
        };

        let Some(notebook) = &self.current_notebook else {
            warn!("Cannot broadcast operation: no notebook open");
            return;
        };

        // Serialize the operation
        let serialized = match postcard::to_stdvec(operation) {
            Ok(bytes) => bytes,
            Err(e) => {
                warn!("Failed to serialize note operation: {}", e);
                return;
            }
        };

        // Send message to all peers in the interface
        match node.send_message(&notebook.interface_id, serialized).await {
            Ok(event_id) => {
                tracing::debug!(
                    "Broadcast operation to interface {}: event_id={}",
                    notebook.interface_id,
                    event_id
                );
            }
            Err(e) => {
                warn!("Failed to broadcast note operation: {}", e);
            }
        }
    }

    /// Save the current notebook to storage
    async fn save_current_notebook(&self) -> Result<(), AppError> {
        if let Some(notebook) = &self.current_notebook {
            self.storage.save_notebook(notebook).await?;
        }
        Ok(())
    }

    /// Close the current notebook
    pub async fn close_notebook(&mut self) -> Result<(), AppError> {
        self.save_current_notebook().await?;
        self.current_notebook = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    async fn create_test_app() -> (App, TempDir) {
        let temp = TempDir::new().unwrap();
        let storage = LocalStorage::new(temp.path()).await.unwrap();
        let app = App::with_storage(storage).await.unwrap();
        (app, temp)
    }

    #[tokio::test]
    async fn test_init_and_load() {
        let (mut app, _temp) = create_test_app().await;

        assert!(!app.is_initialized().await);

        app.init("Alice").await.unwrap();

        assert!(app.is_initialized().await);
        assert_eq!(app.user_name(), Some("Alice"));
    }

    #[tokio::test]
    async fn test_create_notebook() {
        let (mut app, _temp) = create_test_app().await;
        app.init("Alice").await.unwrap();

        let interface_id = app.create_notebook("My Notes").await.unwrap();

        let notebooks = app.list_notebooks().await.unwrap();
        assert_eq!(notebooks.len(), 1);
        assert_eq!(notebooks[0].name, "My Notes");
        assert_eq!(notebooks[0].interface_id, interface_id);
    }

    #[tokio::test]
    async fn test_note_operations() {
        let (mut app, _temp) = create_test_app().await;
        app.init("Alice").await.unwrap();

        let interface_id = app.create_notebook("Test").await.unwrap();
        app.open_notebook(&interface_id).await.unwrap();

        // Create note
        let note_id = app.create_note("First Note").await.unwrap();
        assert!(app.find_note(&note_id).is_some());

        // Update content
        app.update_note_content(&note_id, "Hello world")
            .await
            .unwrap();
        assert_eq!(app.find_note(&note_id).unwrap().content, "Hello world");

        // Delete
        app.delete_note(&note_id).await.unwrap();
        assert!(app.find_note(&note_id).is_none());
    }

    #[tokio::test]
    async fn test_partial_id_match() {
        let (mut app, _temp) = create_test_app().await;
        app.init("Alice").await.unwrap();

        let interface_id = app.create_notebook("Test").await.unwrap();
        app.open_notebook(&interface_id).await.unwrap();

        let note_id = app.create_note("Test Note").await.unwrap();

        // Should find by prefix
        let partial = &note_id[..8];
        assert!(app.find_note(partial).is_some());
    }
}
