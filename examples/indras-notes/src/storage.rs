//! Local storage for notebooks and identity
//!
//! Persists notebooks to disk using JSON files.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use indras_core::InterfaceId;

use crate::notebook::Notebook;

/// Storage errors
#[derive(Debug, Error)]
pub enum StorageError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("Notebook not found: {0}")]
    NotFound(String),
}

/// User profile stored locally
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserProfile {
    /// User's display name
    pub name: String,
    /// Identity bytes (IrohIdentity serialized)
    pub identity_bytes: Vec<u8>,
    /// Secret key bytes for recreating identity
    pub secret_key_bytes: Vec<u8>,
}

impl UserProfile {
    pub fn new(
        name: impl Into<String>,
        identity_bytes: Vec<u8>,
        secret_key_bytes: Vec<u8>,
    ) -> Self {
        Self {
            name: name.into(),
            identity_bytes,
            secret_key_bytes,
        }
    }
}

/// Notebook metadata for listing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotebookMeta {
    pub name: String,
    pub interface_id: InterfaceId,
    pub note_count: usize,
}

/// Index of all notebooks
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct NotebookIndex {
    notebooks: HashMap<String, NotebookMeta>, // interface_id hex -> meta
}

/// Local storage manager
pub struct LocalStorage {
    base_dir: PathBuf,
}

impl LocalStorage {
    /// Create storage at the given directory
    pub async fn new(base_dir: impl Into<PathBuf>) -> Result<Self, StorageError> {
        let base_dir = base_dir.into();
        fs::create_dir_all(&base_dir).await?;
        fs::create_dir_all(base_dir.join("notebooks")).await?;

        Ok(Self { base_dir })
    }

    /// Create storage in the default location (~/.indras-notes)
    pub async fn default_location() -> Result<Self, StorageError> {
        let base_dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".indras-notes");
        Self::new(base_dir).await
    }

    /// Get the profile path
    fn profile_path(&self) -> PathBuf {
        self.base_dir.join("profile.json")
    }

    /// Get the notebook index path
    fn index_path(&self) -> PathBuf {
        self.base_dir.join("notebooks.json")
    }

    /// Get the path for a notebook file
    fn notebook_path(&self, interface_id: &InterfaceId) -> PathBuf {
        self.base_dir
            .join("notebooks")
            .join(format!("{}.json", hex::encode(interface_id.as_bytes())))
    }

    /// Check if a profile exists
    pub async fn has_profile(&self) -> bool {
        self.profile_path().exists()
    }

    /// Save user profile
    pub async fn save_profile(&self, profile: &UserProfile) -> Result<(), StorageError> {
        let json = serde_json::to_string_pretty(profile)?;
        let mut file = fs::File::create(self.profile_path()).await?;
        file.write_all(json.as_bytes()).await?;
        Ok(())
    }

    /// Load user profile
    pub async fn load_profile(&self) -> Result<UserProfile, StorageError> {
        let mut file = fs::File::open(self.profile_path()).await?;
        let mut contents = String::new();
        file.read_to_string(&mut contents).await?;
        Ok(serde_json::from_str(&contents)?)
    }

    /// Load the notebook index
    async fn load_index(&self) -> Result<NotebookIndex, StorageError> {
        let path = self.index_path();
        if !path.exists() {
            return Ok(NotebookIndex::default());
        }

        let mut file = fs::File::open(&path).await?;
        let mut contents = String::new();
        file.read_to_string(&mut contents).await?;
        Ok(serde_json::from_str(&contents)?)
    }

    /// Save the notebook index
    async fn save_index(&self, index: &NotebookIndex) -> Result<(), StorageError> {
        let json = serde_json::to_string_pretty(index)?;
        let mut file = fs::File::create(self.index_path()).await?;
        file.write_all(json.as_bytes()).await?;
        Ok(())
    }

    /// Save a notebook
    pub async fn save_notebook(&self, notebook: &Notebook) -> Result<(), StorageError> {
        // Save the notebook file
        let json = serde_json::to_string_pretty(notebook)?;
        let mut file = fs::File::create(self.notebook_path(&notebook.interface_id)).await?;
        file.write_all(json.as_bytes()).await?;

        // Update index
        let mut index = self.load_index().await?;
        index.notebooks.insert(
            hex::encode(notebook.interface_id.as_bytes()),
            NotebookMeta {
                name: notebook.name.clone(),
                interface_id: notebook.interface_id,
                note_count: notebook.count(),
            },
        );
        self.save_index(&index).await?;

        Ok(())
    }

    /// Load a notebook by interface ID
    pub async fn load_notebook(
        &self,
        interface_id: &InterfaceId,
    ) -> Result<Notebook, StorageError> {
        let path = self.notebook_path(interface_id);
        if !path.exists() {
            return Err(StorageError::NotFound(hex::encode(interface_id.as_bytes())));
        }

        let mut file = fs::File::open(&path).await?;
        let mut contents = String::new();
        file.read_to_string(&mut contents).await?;
        Ok(serde_json::from_str(&contents)?)
    }

    /// List all notebooks
    pub async fn list_notebooks(&self) -> Result<Vec<NotebookMeta>, StorageError> {
        let index = self.load_index().await?;
        Ok(index.notebooks.into_values().collect())
    }

    /// Delete a notebook
    pub async fn delete_notebook(&self, interface_id: &InterfaceId) -> Result<(), StorageError> {
        let path = self.notebook_path(interface_id);
        if path.exists() {
            fs::remove_file(&path).await?;
        }

        // Update index
        let mut index = self.load_index().await?;
        index
            .notebooks
            .remove(&hex::encode(interface_id.as_bytes()));
        self.save_index(&index).await?;

        Ok(())
    }

    /// Get the data directory path
    pub fn data_dir(&self) -> &Path {
        &self.base_dir
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_profile_storage() {
        let temp = TempDir::new().unwrap();
        let storage = LocalStorage::new(temp.path()).await.unwrap();

        assert!(!storage.has_profile().await);

        let profile = UserProfile::new("Alice", vec![1, 2, 3], vec![4, 5, 6]);
        storage.save_profile(&profile).await.unwrap();

        assert!(storage.has_profile().await);

        let loaded = storage.load_profile().await.unwrap();
        assert_eq!(loaded.name, "Alice");
        assert_eq!(loaded.identity_bytes, vec![1, 2, 3]);
    }

    #[tokio::test]
    async fn test_notebook_storage() {
        let temp = TempDir::new().unwrap();
        let storage = LocalStorage::new(temp.path()).await.unwrap();

        let interface_id = InterfaceId::generate();
        let notebook = Notebook::new("Test Notebook", interface_id);

        storage.save_notebook(&notebook).await.unwrap();

        let loaded = storage.load_notebook(&interface_id).await.unwrap();
        assert_eq!(loaded.name, "Test Notebook");
        assert_eq!(loaded.interface_id, interface_id);

        let list = storage.list_notebooks().await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "Test Notebook");
    }
}
