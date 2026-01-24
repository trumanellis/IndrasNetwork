//! Mesh topology definitions for Indra's Network
//!
//! Provides functions to create various network topologies:
//! - Ring: Each peer connected to neighbors
//! - Full mesh: Every peer connected to every other
//! - Random: Configurable connection probability
//! - Custom: Build from edge list

use rand::Rng;
use std::collections::{BTreeMap, BTreeSet};

use crate::types::{PeerId, PeerInterface, PeerState};

/// A mesh network topology
#[derive(Debug)]
pub struct Mesh {
    /// All peers in the network
    pub peers: BTreeMap<PeerId, PeerState>,
    /// Interfaces between connected peers
    pub interfaces: BTreeMap<(PeerId, PeerId), PeerInterface>,
    /// Adjacency list representation for quick lookups
    adjacency: BTreeMap<PeerId, BTreeSet<PeerId>>,
}

impl Clone for Mesh {
    fn clone(&self) -> Self {
        // Clone the mesh but recreate ProphetState for each peer
        // since ProphetState contains RwLock which doesn't implement Clone
        let peers = self.peers.iter().map(|(id, state)| {
            let mut cloned_state = PeerState::new(*id);
            cloned_state.online = state.online;
            cloned_state.connections = state.connections.clone();
            cloned_state.inbox = state.inbox.clone();
            cloned_state.relay_queue = state.relay_queue.clone();
            cloned_state.delivered = state.delivered.clone();
            cloned_state.pending_backprops = state.pending_backprops.clone();
            cloned_state.sequence = state.sequence;
            cloned_state.last_online_tick = state.last_online_tick;
            // Prophet state is freshly created by PeerState::new()

            (*id, cloned_state)
        }).collect();

        Self {
            peers,
            interfaces: self.interfaces.clone(),
            adjacency: self.adjacency.clone(),
        }
    }
}

impl Mesh {
    /// Create an empty mesh
    pub fn new() -> Self {
        Self {
            peers: BTreeMap::new(),
            interfaces: BTreeMap::new(),
            adjacency: BTreeMap::new(),
        }
    }

    /// Add a peer to the mesh
    pub fn add_peer(&mut self, id: PeerId) {
        if let std::collections::btree_map::Entry::Vacant(e) = self.peers.entry(id) {
            e.insert(PeerState::new(id));
            self.adjacency.insert(id, BTreeSet::new());
        }
    }

    /// Add a bidirectional connection between two peers
    pub fn connect(&mut self, a: PeerId, b: PeerId) {
        if a == b {
            return; // No self-loops
        }

        // Ensure both peers exist
        self.add_peer(a);
        self.add_peer(b);

        // Add to adjacency lists
        self.adjacency.get_mut(&a).unwrap().insert(b);
        self.adjacency.get_mut(&b).unwrap().insert(a);

        // Update peer states
        self.peers.get_mut(&a).unwrap().connections.insert(b);
        self.peers.get_mut(&b).unwrap().connections.insert(a);

        // Create interface (normalized key)
        let interface = PeerInterface::new(a, b);
        let key = interface.key();
        self.interfaces.entry(key).or_insert(interface);
    }

    /// Get all neighbors of a peer
    pub fn neighbors(&self, peer: PeerId) -> Option<&BTreeSet<PeerId>> {
        self.adjacency.get(&peer)
    }

    /// Check if two peers are directly connected
    pub fn are_connected(&self, a: PeerId, b: PeerId) -> bool {
        self.adjacency
            .get(&a)
            .map(|neighbors| neighbors.contains(&b))
            .unwrap_or(false)
    }

    /// Get the interface between two peers
    pub fn get_interface(&self, a: PeerId, b: PeerId) -> Option<&PeerInterface> {
        let key = if a < b { (a, b) } else { (b, a) };
        self.interfaces.get(&key)
    }

    /// Get mutable interface between two peers
    pub fn get_interface_mut(&mut self, a: PeerId, b: PeerId) -> Option<&mut PeerInterface> {
        let key = if a < b { (a, b) } else { (b, a) };
        self.interfaces.get_mut(&key)
    }

    /// Get all peer IDs
    pub fn peer_ids(&self) -> Vec<PeerId> {
        self.peers.keys().copied().collect()
    }

    /// Get number of peers
    pub fn peer_count(&self) -> usize {
        self.peers.len()
    }

    /// Get number of connections (edges)
    pub fn edge_count(&self) -> usize {
        self.interfaces.len()
    }

    /// Find mutual peers between two (potentially disconnected) peers
    pub fn mutual_peers(&self, a: PeerId, b: PeerId) -> BTreeSet<PeerId> {
        let a_neighbors = self.adjacency.get(&a);
        let b_neighbors = self.adjacency.get(&b);

        match (a_neighbors, b_neighbors) {
            (Some(an), Some(bn)) => an.intersection(bn).copied().collect(),
            _ => BTreeSet::new(),
        }
    }

    /// Print a simple ASCII visualization of the mesh
    pub fn visualize(&self) -> String {
        let mut output = String::new();
        output.push_str("Mesh Topology:\n");
        output.push_str(&format!("  Peers: {}\n", self.peer_count()));
        output.push_str(&format!("  Edges: {}\n\n", self.edge_count()));

        for (peer_id, neighbors) in &self.adjacency {
            let neighbor_str: Vec<String> = neighbors.iter().map(|n| n.to_string()).collect();
            output.push_str(&format!("  {} -> [{}]\n", peer_id, neighbor_str.join(", ")));
        }
        output
    }
}

impl Default for Mesh {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for creating mesh topologies
pub struct MeshBuilder {
    peer_count: usize,
}

impl MeshBuilder {
    /// Create a builder with the given number of peers (A, B, C, ...)
    pub fn new(peer_count: usize) -> Self {
        assert!(peer_count <= 26, "Maximum 26 peers (A-Z)");
        Self { peer_count }
    }

    /// Build a ring topology where each peer is connected to its neighbors
    ///
    /// A - B - C - D - ... - Z - A
    pub fn ring(self) -> Mesh {
        let mut mesh = Mesh::new();
        let peers = PeerId::range_to((b'A' + self.peer_count as u8 - 1) as char);

        for peer in &peers {
            mesh.add_peer(*peer);
        }

        for i in 0..peers.len() {
            let next = (i + 1) % peers.len();
            mesh.connect(peers[i], peers[next]);
        }

        mesh
    }

    /// Build a full mesh where every peer is connected to every other
    pub fn full_mesh(self) -> Mesh {
        let mut mesh = Mesh::new();
        let peers = PeerId::range_to((b'A' + self.peer_count as u8 - 1) as char);

        for peer in &peers {
            mesh.add_peer(*peer);
        }

        for i in 0..peers.len() {
            for j in (i + 1)..peers.len() {
                mesh.connect(peers[i], peers[j]);
            }
        }

        mesh
    }

    /// Build a random mesh with given connection probability
    pub fn random(self, connection_probability: f64) -> Mesh {
        let mut mesh = Mesh::new();
        let mut rng = rand::rng();
        let peers = PeerId::range_to((b'A' + self.peer_count as u8 - 1) as char);

        for peer in &peers {
            mesh.add_peer(*peer);
        }

        for i in 0..peers.len() {
            for j in (i + 1)..peers.len() {
                if rng.random::<f64>() < connection_probability {
                    mesh.connect(peers[i], peers[j]);
                }
            }
        }

        // Ensure connectivity: add edges to any isolated nodes
        for peer in &peers {
            if mesh.neighbors(*peer).map(|n| n.is_empty()).unwrap_or(true) {
                // Connect to a random other peer
                let other_idx = rng.random_range(0..peers.len());
                if peers[other_idx] != *peer {
                    mesh.connect(*peer, peers[other_idx]);
                }
            }
        }

        mesh
    }

    /// Build a line topology: A - B - C - D - ...
    pub fn line(self) -> Mesh {
        let mut mesh = Mesh::new();
        let peers = PeerId::range_to((b'A' + self.peer_count as u8 - 1) as char);

        for peer in &peers {
            mesh.add_peer(*peer);
        }

        for i in 0..(peers.len() - 1) {
            mesh.connect(peers[i], peers[i + 1]);
        }

        mesh
    }

    /// Build a star topology: A in center, connected to all others
    pub fn star(self) -> Mesh {
        let mut mesh = Mesh::new();
        let peers = PeerId::range_to((b'A' + self.peer_count as u8 - 1) as char);

        for peer in &peers {
            mesh.add_peer(*peer);
        }

        let center = peers[0];
        for peer in peers.iter().skip(1) {
            mesh.connect(center, *peer);
        }

        mesh
    }
}

/// Create a custom mesh from an edge list
pub fn from_edges(edges: &[(char, char)]) -> Mesh {
    let mut mesh = Mesh::new();

    for (a, b) in edges {
        let peer_a = PeerId::new(*a).expect("Invalid peer ID");
        let peer_b = PeerId::new(*b).expect("Invalid peer ID");
        mesh.connect(peer_a, peer_b);
    }

    mesh
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ring_topology() {
        let mesh = MeshBuilder::new(4).ring();
        assert_eq!(mesh.peer_count(), 4);
        assert_eq!(mesh.edge_count(), 4); // A-B, B-C, C-D, D-A

        assert!(mesh.are_connected(PeerId('A'), PeerId('B')));
        assert!(mesh.are_connected(PeerId('D'), PeerId('A'))); // Wrap around
        assert!(!mesh.are_connected(PeerId('A'), PeerId('C'))); // Not direct
    }

    #[test]
    fn test_full_mesh() {
        let mesh = MeshBuilder::new(4).full_mesh();
        assert_eq!(mesh.peer_count(), 4);
        assert_eq!(mesh.edge_count(), 6); // C(4,2) = 6

        // Everyone connected to everyone
        for a in PeerId::range_to('D') {
            for b in PeerId::range_to('D') {
                if a != b {
                    assert!(mesh.are_connected(a, b));
                }
            }
        }
    }

    #[test]
    fn test_mutual_peers() {
        // A - B - C (line)
        let mesh = from_edges(&[('A', 'B'), ('B', 'C')]);

        // A and C share B as a mutual peer
        let mutual = mesh.mutual_peers(PeerId('A'), PeerId('C'));
        assert_eq!(mutual.len(), 1);
        assert!(mutual.contains(&PeerId('B')));
    }

    #[test]
    fn test_custom_topology() {
        let mesh = from_edges(&[('A', 'B'), ('A', 'C'), ('B', 'C'), ('B', 'D')]);

        assert_eq!(mesh.peer_count(), 4);
        assert_eq!(mesh.edge_count(), 4);
        assert!(mesh.are_connected(PeerId('A'), PeerId('B')));
        assert!(!mesh.are_connected(PeerId('A'), PeerId('D')));
    }
}
