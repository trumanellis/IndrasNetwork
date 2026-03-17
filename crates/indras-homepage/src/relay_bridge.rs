//! File-based homepage persistence for offline profile serving.

use std::path::{Path, PathBuf};

use crate::{HomepageError, HomepageStore, ProfileFieldArtifact};

/// File-based implementation of [`HomepageStore`].
///
/// Persists profile snapshots as versioned JSON files and artifacts as raw bytes.
/// Suitable for local operation where the homepage server and steward
/// run on the same machine.
pub struct FileHomepageStore {
    base_dir: PathBuf,
}

impl FileHomepageStore {
    /// Create a new file-based store at the given directory.
    ///
    /// Creates the directory structure if it doesn't exist.
    pub fn new(base_dir: impl AsRef<Path>) -> Result<Self, HomepageError> {
        let base_dir = base_dir.as_ref().to_path_buf();
        std::fs::create_dir_all(&base_dir)
            .map_err(|e| HomepageError::Storage(format!("Failed to create store dir: {e}")))?;
        std::fs::create_dir_all(base_dir.join("artifacts"))
            .map_err(|e| HomepageError::Storage(format!("Failed to create artifacts dir: {e}")))?;
        Ok(Self { base_dir })
    }

    fn profile_path(&self) -> PathBuf {
        self.base_dir.join("profile.json")
    }

    fn artifact_path(&self, id: &[u8; 32]) -> PathBuf {
        self.base_dir.join("artifacts").join(hex::encode(id))
    }
}

impl HomepageStore for FileHomepageStore {
    fn load_profile(&self) -> Result<Vec<ProfileFieldArtifact>, HomepageError> {
        let path = self.profile_path();
        if !path.exists() {
            return Ok(Vec::new());
        }
        let data = std::fs::read(&path)
            .map_err(|e| HomepageError::Storage(format!("Failed to read profile: {e}")))?;
        if data.is_empty() {
            return Ok(Vec::new());
        }
        // Versioned format: first byte is version
        let version = data[0];
        match version {
            1 => serde_json::from_slice(&data[1..])
                .map_err(|e| HomepageError::Storage(format!("Failed to parse profile: {e}"))),
            _ => Err(HomepageError::Storage(format!(
                "Unknown profile version: {version}"
            ))),
        }
    }

    fn save_profile(&self, fields: &[ProfileFieldArtifact]) -> Result<(), HomepageError> {
        let json = serde_json::to_vec(fields)
            .map_err(|e| HomepageError::Storage(format!("Failed to serialize profile: {e}")))?;
        // Versioned format: prepend version byte
        let mut data = Vec::with_capacity(1 + json.len());
        data.push(1u8); // version 1
        data.extend_from_slice(&json);
        std::fs::write(self.profile_path(), &data)
            .map_err(|e| HomepageError::Storage(format!("Failed to write profile: {e}")))?;
        Ok(())
    }

    fn load_artifact(&self, id: &[u8; 32]) -> Result<Vec<u8>, HomepageError> {
        let path = self.artifact_path(id);
        std::fs::read(&path)
            .map_err(|e| HomepageError::Storage(format!("Failed to read artifact: {e}")))
    }

    fn save_artifact(&self, id: &[u8; 32], data: &[u8]) -> Result<(), HomepageError> {
        std::fs::write(self.artifact_path(id), data)
            .map_err(|e| HomepageError::Storage(format!("Failed to write artifact: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_profile() {
        let dir = std::env::temp_dir().join("indras-homepage-test-profile");
        let _ = std::fs::remove_dir_all(&dir);
        let store = FileHomepageStore::new(&dir).unwrap();

        // Empty initially
        let loaded = store.load_profile().unwrap();
        assert!(loaded.is_empty());

        // Save and reload
        let fields = vec![ProfileFieldArtifact {
            field_name: "display_name".to_string(),
            display_value: "Alice".to_string(),
            grants: Vec::new(),
        }];
        store.save_profile(&fields).unwrap();

        let loaded = store.load_profile().unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].field_name, "display_name");
        assert_eq!(loaded[0].display_value, "Alice");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn round_trip_artifact() {
        let dir = std::env::temp_dir().join("indras-homepage-test-artifact");
        let _ = std::fs::remove_dir_all(&dir);
        let store = FileHomepageStore::new(&dir).unwrap();

        let id = [42u8; 32];
        let data = b"hello world";
        store.save_artifact(&id, data).unwrap();

        let loaded = store.load_artifact(&id).unwrap();
        assert_eq!(loaded, data);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
