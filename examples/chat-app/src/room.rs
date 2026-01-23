//! Chat room management
//!
//! Provides a simplified chat room abstraction that can work in demo mode
//! (single-user with simulated peers) or be extended for real networking.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Chat room errors
#[derive(Debug, Error)]
pub enum RoomError {
    #[error("Room not found: {0}")]
    NotFound(String),

    #[error("Room already exists: {0}")]
    AlreadyExists(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

/// A chat message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    /// Message ID (unique within room)
    pub id: u64,
    /// Sender's display name
    pub sender: String,
    /// Message content
    pub content: String,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
    /// Whether this is a system message
    pub is_system: bool,
}

impl ChatMessage {
    /// Create a new user message
    pub fn new(id: u64, sender: &str, content: &str) -> Self {
        Self {
            id,
            sender: sender.to_string(),
            content: content.to_string(),
            timestamp: Utc::now(),
            is_system: false,
        }
    }

    /// Create a system message
    pub fn system(id: u64, content: &str) -> Self {
        Self {
            id,
            sender: "system".to_string(),
            content: content.to_string(),
            timestamp: Utc::now(),
            is_system: true,
        }
    }
}

/// Chat room metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomMeta {
    /// Room ID (derived from interface ID in real mode)
    pub id: String,
    /// Room display name
    pub name: String,
    /// Room creator
    pub creator: String,
    /// Creation time
    pub created_at: DateTime<Utc>,
    /// Last activity time
    pub last_activity: DateTime<Utc>,
    /// Known members
    pub members: Vec<String>,
}

impl RoomMeta {
    /// Create new room metadata
    pub fn new(name: &str, creator: &str) -> Self {
        let id = generate_room_id();
        let now = Utc::now();

        Self {
            id,
            name: name.to_string(),
            creator: creator.to_string(),
            created_at: now,
            last_activity: now,
            members: vec![creator.to_string()],
        }
    }

    /// Create from an existing ID (for joining)
    pub fn from_id(id: &str, name: &str, joiner: &str) -> Self {
        let now = Utc::now();

        Self {
            id: id.to_string(),
            name: name.to_string(),
            creator: "Unknown".to_string(),
            created_at: now,
            last_activity: now,
            members: vec![joiner.to_string()],
        }
    }
}

/// A chat room with message history
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRoom {
    /// Room metadata
    pub meta: RoomMeta,
    /// Message history
    pub messages: Vec<ChatMessage>,
    /// Next message ID
    next_id: u64,
}

impl ChatRoom {
    /// Create a new chat room
    pub fn new(name: &str, creator: &str) -> Self {
        let meta = RoomMeta::new(name, creator);
        let mut room = Self {
            meta,
            messages: Vec::new(),
            next_id: 1,
        };

        // Add welcome message
        room.add_system_message(&format!("{} created the room", creator));

        room
    }

    /// Create a room from an ID (for joining)
    pub fn from_id(id: &str, name: &str, joiner: &str) -> Self {
        let meta = RoomMeta::from_id(id, name, joiner);
        let mut room = Self {
            meta,
            messages: Vec::new(),
            next_id: 1,
        };

        // Add join message
        room.add_system_message(&format!("{} joined the room", joiner));

        room
    }

    /// Get room ID
    pub fn id(&self) -> &str {
        &self.meta.id
    }

    /// Get room name
    pub fn name(&self) -> &str {
        &self.meta.name
    }

    /// Get members
    pub fn members(&self) -> &[String] {
        &self.meta.members
    }

    /// Get message count
    pub fn message_count(&self) -> usize {
        self.messages.len()
    }

    /// Add a user message
    pub fn add_message(&mut self, sender: &str, content: &str) -> &ChatMessage {
        let msg = ChatMessage::new(self.next_id, sender, content);
        self.next_id += 1;
        self.meta.last_activity = Utc::now();

        // Track member if new
        if !self.meta.members.contains(&sender.to_string()) {
            self.meta.members.push(sender.to_string());
        }

        self.messages.push(msg);
        self.messages.last().unwrap()
    }

    /// Add a system message
    pub fn add_system_message(&mut self, content: &str) -> &ChatMessage {
        let msg = ChatMessage::system(self.next_id, content);
        self.next_id += 1;
        self.meta.last_activity = Utc::now();

        self.messages.push(msg);
        self.messages.last().unwrap()
    }

    /// Get recent messages
    pub fn recent_messages(&self, count: usize) -> &[ChatMessage] {
        let start = self.messages.len().saturating_sub(count);
        &self.messages[start..]
    }

    /// Get all messages
    pub fn all_messages(&self) -> &[ChatMessage] {
        &self.messages
    }
}

/// Room storage manager
pub struct RoomStorage {
    data_dir: PathBuf,
    rooms: HashMap<String, ChatRoom>,
}

impl RoomStorage {
    /// Create a new room storage
    pub fn new(data_dir: PathBuf) -> Result<Self, RoomError> {
        fs::create_dir_all(data_dir.join("rooms"))?;

        let mut storage = Self {
            data_dir,
            rooms: HashMap::new(),
        };

        // Load existing rooms
        storage.load_all()?;

        Ok(storage)
    }

    /// Load all rooms from disk
    fn load_all(&mut self) -> Result<(), RoomError> {
        let rooms_dir = self.data_dir.join("rooms");

        if !rooms_dir.exists() {
            return Ok(());
        }

        for entry in fs::read_dir(&rooms_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().map_or(false, |e| e == "json") {
                let content = fs::read_to_string(&path)?;
                if let Ok(room) = serde_json::from_str::<ChatRoom>(&content) {
                    self.rooms.insert(room.meta.id.clone(), room);
                }
            }
        }

        Ok(())
    }

    /// Save a room to disk
    pub fn save(&self, room_id: &str) -> Result<(), RoomError> {
        let room = self.rooms.get(room_id).ok_or_else(|| RoomError::NotFound(room_id.to_string()))?;

        let rooms_dir = self.data_dir.join("rooms");
        let path = rooms_dir.join(format!("{}.json", room_id));

        fs::write(&path, serde_json::to_string_pretty(room)?)?;

        Ok(())
    }

    /// Save all rooms
    pub fn save_all(&self) -> Result<(), RoomError> {
        for room_id in self.rooms.keys() {
            self.save(room_id)?;
        }
        Ok(())
    }

    /// Create a new room
    pub fn create(&mut self, name: &str, creator: &str) -> Result<&ChatRoom, RoomError> {
        let room = ChatRoom::new(name, creator);
        let room_id = room.id().to_string();

        self.rooms.insert(room_id.clone(), room);
        self.save(&room_id)?;

        Ok(self.rooms.get(&room_id).unwrap())
    }

    /// Join an existing room by ID
    pub fn join(&mut self, id: &str, name: &str, joiner: &str) -> Result<&ChatRoom, RoomError> {
        // Check if we already have this room
        if self.rooms.contains_key(id) {
            // Just add a join message
            let room = self.rooms.get_mut(id).unwrap();
            room.add_system_message(&format!("{} rejoined the room", joiner));
            self.save(id)?;
            return Ok(self.rooms.get(id).unwrap());
        }

        let room = ChatRoom::from_id(id, name, joiner);
        let room_id = room.id().to_string();

        self.rooms.insert(room_id.clone(), room);
        self.save(&room_id)?;

        Ok(self.rooms.get(&room_id).unwrap())
    }

    /// Get a room by ID
    pub fn get(&self, id: &str) -> Option<&ChatRoom> {
        self.rooms.get(id)
    }

    /// Get a mutable room by ID
    pub fn get_mut(&mut self, id: &str) -> Option<&mut ChatRoom> {
        self.rooms.get_mut(id)
    }

    /// List all rooms
    pub fn list(&self) -> Vec<&ChatRoom> {
        let mut rooms: Vec<_> = self.rooms.values().collect();
        rooms.sort_by(|a, b| b.meta.last_activity.cmp(&a.meta.last_activity));
        rooms
    }

    /// Find room by partial ID or name
    pub fn find(&self, query: &str) -> Option<&ChatRoom> {
        // Try exact ID match
        if let Some(room) = self.rooms.get(query) {
            return Some(room);
        }

        // Try by index
        if let Ok(index) = query.parse::<usize>() {
            let rooms = self.list();
            if index > 0 && index <= rooms.len() {
                return Some(rooms[index - 1]);
            }
        }

        // Try partial ID match
        for room in self.rooms.values() {
            if room.id().starts_with(query) {
                return Some(room);
            }
        }

        // Try name match
        let query_lower = query.to_lowercase();
        for room in self.rooms.values() {
            if room.name().to_lowercase().contains(&query_lower) {
                return Some(room);
            }
        }

        None
    }

    /// Delete a room
    pub fn delete(&mut self, id: &str) -> Result<(), RoomError> {
        if self.rooms.remove(id).is_none() {
            return Err(RoomError::NotFound(id.to_string()));
        }

        let path = self.data_dir.join("rooms").join(format!("{}.json", id));
        if path.exists() {
            fs::remove_file(&path)?;
        }

        Ok(())
    }
}

/// Generate a random room ID
fn generate_room_id() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 16];
    rand::rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_chat_message_creation() {
        let msg = ChatMessage::new(1, "Alice", "Hello world");
        assert_eq!(msg.id, 1);
        assert_eq!(msg.sender, "Alice");
        assert_eq!(msg.content, "Hello world");
        assert!(!msg.is_system);
    }

    #[test]
    fn test_system_message() {
        let msg = ChatMessage::system(1, "User joined");
        assert!(msg.is_system);
        assert_eq!(msg.sender, "system");
    }

    #[test]
    fn test_room_creation() {
        let room = ChatRoom::new("Test Room", "Alice");
        assert_eq!(room.name(), "Test Room");
        assert!(room.members().contains(&"Alice".to_string()));
        // Should have welcome message
        assert!(!room.messages.is_empty());
    }

    #[test]
    fn test_add_message() {
        let mut room = ChatRoom::new("Test", "Alice");
        let initial_count = room.message_count();

        room.add_message("Alice", "Hello");
        room.add_message("Bob", "Hi there");

        assert_eq!(room.message_count(), initial_count + 2);
        assert!(room.members().contains(&"Bob".to_string()));
    }

    #[test]
    fn test_room_storage() {
        let dir = tempdir().unwrap();
        let mut storage = RoomStorage::new(dir.path().to_path_buf()).unwrap();

        // Create a room
        let room = storage.create("Test Room", "Alice").unwrap();
        let room_id = room.id().to_string();

        // Should be findable
        assert!(storage.get(&room_id).is_some());

        // List should include it
        assert!(!storage.list().is_empty());
    }

    #[test]
    fn test_room_persistence() {
        let dir = tempdir().unwrap();
        let room_id: String;

        {
            let mut storage = RoomStorage::new(dir.path().to_path_buf()).unwrap();
            let room = storage.create("Persistent Room", "Alice").unwrap();
            room_id = room.id().to_string();
        }

        // Reload storage
        let storage = RoomStorage::new(dir.path().to_path_buf()).unwrap();
        let room = storage.get(&room_id).unwrap();
        assert_eq!(room.name(), "Persistent Room");
    }

    #[test]
    fn test_find_by_partial() {
        let dir = tempdir().unwrap();
        let mut storage = RoomStorage::new(dir.path().to_path_buf()).unwrap();

        storage.create("Alpha Room", "Alice").unwrap();
        storage.create("Beta Room", "Bob").unwrap();

        // Find by name
        assert!(storage.find("alpha").is_some());
        assert!(storage.find("Beta").is_some());

        // Find by index
        assert!(storage.find("1").is_some());
        assert!(storage.find("2").is_some());
    }
}
