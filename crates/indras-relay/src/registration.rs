//! Registration state management
//!
//! Tracks which peers are registered for which interfaces,
//! with JSON persistence to disk.

use std::collections::HashSet;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use indras_core::InterfaceId;
use indras_core::identity::PeerIdentity;
use indras_transport::identity::IrohIdentity;

use crate::error::{RelayError, RelayResult};

/// A peer's registration with the relay
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerRegistration {
    /// The peer's identity (serialized as bytes)
    pub peer_id_bytes: Vec<u8>,
    /// Optional display name
    pub display_name: Option<String>,
    /// Registered interfaces (serialized as byte arrays)
    pub interface_bytes: Vec<Vec<u8>>,
    /// When the peer first registered (Unix millis)
    pub registered_at_millis: i64,
    /// When the peer was last seen (Unix millis)
    pub last_seen_millis: i64,
}

/// Manages registration state
pub struct RegistrationState {
    /// Registered peers → their registration info
    peers: DashMap<IrohIdentity, PeerRegistrationInfo>,
    /// Interface → set of peer identity bytes registered for it
    interface_subscribers: DashMap<InterfaceId, HashSet<Vec<u8>>>,
    /// Persistence path
    state_path: PathBuf,
}

/// In-memory registration info
#[derive(Debug, Clone)]
pub struct PeerRegistrationInfo {
    pub peer_id: IrohIdentity,
    pub display_name: Option<String>,
    pub interfaces: Vec<InterfaceId>,
    pub registered_at: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
}

/// Serialized state for persistence
#[derive(Debug, Serialize, Deserialize)]
struct PersistedState {
    registrations: Vec<PeerRegistration>,
}

impl RegistrationState {
    /// Create a new registration state with the given persistence path
    pub fn new(state_path: PathBuf) -> Self {
        Self {
            peers: DashMap::new(),
            interface_subscribers: DashMap::new(),
            state_path,
        }
    }

    /// Load state from disk if it exists
    pub fn load(&self) -> RelayResult<()> {
        if !self.state_path.exists() {
            return Ok(());
        }

        let data = std::fs::read_to_string(&self.state_path).map_err(|e| {
            RelayError::Registration(format!("Failed to read state: {e}"))
        })?;

        let state: PersistedState = serde_json::from_str(&data).map_err(|e| {
            RelayError::Registration(format!("Failed to parse state: {e}"))
        })?;

        for reg in state.registrations {
            let peer_id = IrohIdentity::from_bytes(&reg.peer_id_bytes).map_err(|e| {
                RelayError::Registration(format!("Invalid peer ID: {e}"))
            })?;

            let interfaces: Vec<InterfaceId> = reg
                .interface_bytes
                .iter()
                .filter_map(|b| {
                    if b.len() == 32 {
                        let mut arr = [0u8; 32];
                        arr.copy_from_slice(b);
                        Some(InterfaceId::new(arr))
                    } else {
                        None
                    }
                })
                .collect();

            let info = PeerRegistrationInfo {
                peer_id,
                display_name: reg.display_name.clone(),
                interfaces: interfaces.clone(),
                registered_at: DateTime::from_timestamp_millis(reg.registered_at_millis)
                    .unwrap_or_else(Utc::now),
                last_seen: DateTime::from_timestamp_millis(reg.last_seen_millis)
                    .unwrap_or_else(Utc::now),
            };

            // Rebuild interface_subscribers
            for iface in &interfaces {
                self.interface_subscribers
                    .entry(*iface)
                    .or_insert_with(HashSet::new)
                    .insert(reg.peer_id_bytes.clone());
            }

            self.peers.insert(peer_id, info);
        }

        info!(
            peers = self.peers.len(),
            interfaces = self.interface_subscribers.len(),
            "Loaded registration state"
        );
        Ok(())
    }

    /// Save state to disk
    pub fn save(&self) -> RelayResult<()> {
        let registrations: Vec<PeerRegistration> = self
            .peers
            .iter()
            .map(|entry| {
                let info = entry.value();
                PeerRegistration {
                    peer_id_bytes: info.peer_id.as_bytes().to_vec(),
                    display_name: info.display_name.clone(),
                    interface_bytes: info
                        .interfaces
                        .iter()
                        .map(|i| i.0.to_vec())
                        .collect(),
                    registered_at_millis: info.registered_at.timestamp_millis(),
                    last_seen_millis: info.last_seen.timestamp_millis(),
                }
            })
            .collect();

        let state = PersistedState { registrations };
        let data = serde_json::to_string_pretty(&state).map_err(|e| {
            RelayError::Registration(format!("Failed to serialize state: {e}"))
        })?;

        if let Some(parent) = self.state_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                RelayError::Registration(format!("Failed to create state directory: {e}"))
            })?;
        }

        std::fs::write(&self.state_path, data).map_err(|e| {
            RelayError::Registration(format!("Failed to write state: {e}"))
        })?;

        debug!("Saved registration state");
        Ok(())
    }

    /// Register a peer for interfaces
    pub fn register(
        &self,
        peer_id: IrohIdentity,
        interfaces: Vec<InterfaceId>,
        display_name: Option<String>,
    ) -> RelayResult<()> {
        let now = Utc::now();
        let peer_bytes = peer_id.as_bytes().to_vec();

        // Update or create registration
        self.peers
            .entry(peer_id)
            .and_modify(|info| {
                // Add new interfaces (deduplicate)
                for iface in &interfaces {
                    if !info.interfaces.contains(iface) {
                        info.interfaces.push(*iface);
                    }
                }
                if let Some(ref name) = display_name {
                    info.display_name = Some(name.clone());
                }
                info.last_seen = now;
            })
            .or_insert(PeerRegistrationInfo {
                peer_id,
                display_name,
                interfaces: interfaces.clone(),
                registered_at: now,
                last_seen: now,
            });

        // Update interface → peer mapping
        for iface in &interfaces {
            self.interface_subscribers
                .entry(*iface)
                .or_insert_with(HashSet::new)
                .insert(peer_bytes.clone());
        }

        self.save()?;
        Ok(())
    }

    /// Unregister a peer from interfaces
    pub fn unregister(
        &self,
        peer_id: &IrohIdentity,
        interfaces: &[InterfaceId],
    ) -> RelayResult<()> {
        let peer_bytes = peer_id.as_bytes().to_vec();

        if let Some(mut info) = self.peers.get_mut(peer_id) {
            info.interfaces.retain(|i| !interfaces.contains(i));
        }

        for iface in interfaces {
            if let Some(mut subs) = self.interface_subscribers.get_mut(iface) {
                subs.remove(&peer_bytes);
                if subs.is_empty() {
                    drop(subs);
                    self.interface_subscribers.remove(iface);
                }
            }
        }

        self.save()?;
        Ok(())
    }

    /// Update last seen time for a peer
    pub fn touch(&self, peer_id: &IrohIdentity) {
        if let Some(mut info) = self.peers.get_mut(peer_id) {
            info.last_seen = Utc::now();
        }
    }

    /// Check if an interface has any registered peers
    pub fn is_interface_registered(&self, interface_id: &InterfaceId) -> bool {
        self.interface_subscribers
            .get(interface_id)
            .map(|s| !s.is_empty())
            .unwrap_or(false)
    }

    /// Get all interfaces that have registered peers
    pub fn registered_interfaces(&self) -> Vec<InterfaceId> {
        self.interface_subscribers
            .iter()
            .map(|entry| *entry.key())
            .collect()
    }

    /// Get all registered peers
    pub fn registered_peers(&self) -> Vec<PeerRegistrationInfo> {
        self.peers.iter().map(|entry| entry.value().clone()).collect()
    }

    /// Get the number of registered peers
    pub fn peer_count(&self) -> usize {
        self.peers.len()
    }

    /// Get the number of registered interfaces
    pub fn interface_count(&self) -> usize {
        self.interface_subscribers.len()
    }

    /// Get interfaces registered by a specific peer
    pub fn peer_interfaces(&self, peer_id: &IrohIdentity) -> Vec<InterfaceId> {
        self.peers
            .get(peer_id)
            .map(|info| info.interfaces.clone())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iroh::SecretKey;
    use tempfile::TempDir;

    fn test_peer() -> IrohIdentity {
        let secret = SecretKey::generate(&mut rand::rng());
        IrohIdentity::new(secret.public())
    }

    fn test_state() -> (RegistrationState, TempDir) {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("registrations.json");
        let state = RegistrationState::new(path);
        (state, dir)
    }

    #[test]
    fn test_register_and_lookup() {
        let (state, _dir) = test_state();
        let peer = test_peer();
        let iface1 = InterfaceId::new([0x11; 32]);
        let iface2 = InterfaceId::new([0x22; 32]);

        state
            .register(peer, vec![iface1, iface2], Some("TestPeer".into()))
            .unwrap();

        assert!(state.is_interface_registered(&iface1));
        assert!(state.is_interface_registered(&iface2));
        assert!(!state.is_interface_registered(&InterfaceId::new([0x33; 32])));

        let interfaces = state.peer_interfaces(&peer);
        assert_eq!(interfaces.len(), 2);
        assert!(interfaces.contains(&iface1));
        assert!(interfaces.contains(&iface2));
    }

    #[test]
    fn test_unregister() {
        let (state, _dir) = test_state();
        let peer = test_peer();
        let iface1 = InterfaceId::new([0x11; 32]);
        let iface2 = InterfaceId::new([0x22; 32]);

        state.register(peer, vec![iface1, iface2], None).unwrap();
        state.unregister(&peer, &[iface1]).unwrap();

        assert!(!state.is_interface_registered(&iface1));
        assert!(state.is_interface_registered(&iface2));
    }

    #[test]
    fn test_persistence_roundtrip() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("registrations.json");

        let peer = test_peer();
        let iface = InterfaceId::new([0x42; 32]);

        // Save
        {
            let state = RegistrationState::new(path.clone());
            state
                .register(peer, vec![iface], Some("Persistent".into()))
                .unwrap();
        }

        // Load
        {
            let state = RegistrationState::new(path);
            state.load().unwrap();

            assert!(state.is_interface_registered(&iface));
            assert_eq!(state.peer_count(), 1);
            let peers = state.registered_peers();
            assert_eq!(peers[0].display_name, Some("Persistent".to_string()));
        }
    }

    #[test]
    fn test_multiple_peers_same_interface() {
        let (state, _dir) = test_state();
        let peer1 = test_peer();
        let peer2 = test_peer();
        let iface = InterfaceId::new([0x42; 32]);

        state.register(peer1, vec![iface], None).unwrap();
        state.register(peer2, vec![iface], None).unwrap();

        assert_eq!(state.peer_count(), 2);
        assert_eq!(state.interface_count(), 1);

        // Unregister one peer — interface should still be registered
        state.unregister(&peer1, &[iface]).unwrap();
        assert!(state.is_interface_registered(&iface));

        // Unregister the other — interface should no longer be registered
        state.unregister(&peer2, &[iface]).unwrap();
        assert!(!state.is_interface_registered(&iface));
    }
}
