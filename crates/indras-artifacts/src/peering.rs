use serde::{Deserialize, Serialize};

use crate::artifact::PlayerId;
use crate::error::VaultError;

type Result<T> = std::result::Result<T, VaultError>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerEntry {
    pub peer_id: PlayerId,
    pub since: i64,
    pub display_name: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerRegistry {
    pub player: PlayerId,
    pub peers: Vec<PeerEntry>,
}

impl PeerRegistry {
    pub fn new(player: PlayerId) -> Self {
        Self {
            player,
            peers: Vec::new(),
        }
    }

    pub fn add_peer(
        &mut self,
        peer_id: PlayerId,
        display_name: Option<String>,
        now: i64,
    ) -> Result<()> {
        if self.is_peer(&peer_id) {
            return Err(VaultError::AlreadyPeered);
        }
        self.peers.push(PeerEntry {
            peer_id,
            since: now,
            display_name,
        });
        Ok(())
    }

    pub fn remove_peer(&mut self, peer_id: &PlayerId) -> Result<()> {
        let before = self.peers.len();
        self.peers.retain(|p| &p.peer_id != peer_id);
        if self.peers.len() == before {
            return Err(VaultError::NotPeered);
        }
        Ok(())
    }

    pub fn is_peer(&self, peer_id: &PlayerId) -> bool {
        self.peers.iter().any(|p| &p.peer_id == peer_id)
    }

    pub fn peers(&self) -> &[PeerEntry] {
        &self.peers
    }

    pub fn peer_count(&self) -> usize {
        self.peers.len()
    }
}

/// Canonical representation of a mutual peering relationship.
/// peer_a < peer_b (lexicographically sorted).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MutualPeering {
    pub peer_a: PlayerId,
    pub peer_b: PlayerId,
    pub since: i64,
}

impl MutualPeering {
    /// Create with canonical ordering (sorted).
    pub fn new(a: PlayerId, b: PlayerId, since: i64) -> Self {
        if a <= b {
            Self {
                peer_a: a,
                peer_b: b,
                since,
            }
        } else {
            Self {
                peer_a: b,
                peer_b: a,
                since,
            }
        }
    }

    pub fn contains(&self, player: &PlayerId) -> bool {
        &self.peer_a == player || &self.peer_b == player
    }

    pub fn other(&self, player: &PlayerId) -> Option<&PlayerId> {
        if &self.peer_a == player {
            Some(&self.peer_b)
        } else if &self.peer_b == player {
            Some(&self.peer_a)
        } else {
            None
        }
    }
}
