//! Bridge module connecting simulation types to indras-core traits
//!
//! This module provides trait implementations that allow the simulation
//! to use the real routing and storage implementations from indras-routing
//! and indras-storage.

use indras_core::{NetworkTopology, SimulationIdentity};

use crate::topology::Mesh;
use crate::types::PeerId;

/// Convert simulation PeerId to core SimulationIdentity
impl From<PeerId> for SimulationIdentity {
    fn from(peer: PeerId) -> Self {
        SimulationIdentity(peer.0)
    }
}

/// Convert core SimulationIdentity to simulation PeerId
impl From<SimulationIdentity> for PeerId {
    fn from(identity: SimulationIdentity) -> Self {
        PeerId(identity.0)
    }
}

/// Implement NetworkTopology trait for Mesh
///
/// This allows the simulation's Mesh to be used with the real
/// StoreForwardRouter from indras-routing.
impl NetworkTopology<SimulationIdentity> for Mesh {
    fn peers(&self) -> Vec<SimulationIdentity> {
        self.peers.keys().map(|p| (*p).into()).collect()
    }

    fn neighbors(&self, peer: &SimulationIdentity) -> Vec<SimulationIdentity> {
        let peer_id: PeerId = (*peer).into();
        Mesh::neighbors(self, peer_id)
            .map(|neighbors| neighbors.iter().map(|p| (*p).into()).collect())
            .unwrap_or_default()
    }

    fn are_connected(&self, a: &SimulationIdentity, b: &SimulationIdentity) -> bool {
        let peer_a: PeerId = (*a).into();
        let peer_b: PeerId = (*b).into();
        Mesh::are_connected(self, peer_a, peer_b)
    }

    fn is_online(&self, peer: &SimulationIdentity) -> bool {
        let peer_id: PeerId = (*peer).into();
        self.peers
            .get(&peer_id)
            .map(|state| state.online)
            .unwrap_or(false)
    }
}

/// Helper trait for using Mesh with SimulationIdentity
pub trait MeshBridge {
    /// Get all online peers as SimulationIdentity
    fn online_peers_sim(&self) -> Vec<SimulationIdentity>;

    /// Check if a SimulationIdentity peer is online
    fn is_online_sim(&self, peer: &SimulationIdentity) -> bool;

    /// Get neighbors as SimulationIdentity
    fn neighbors_sim(&self, peer: &SimulationIdentity) -> Vec<SimulationIdentity>;

    /// Get mutual peers as SimulationIdentity
    fn mutual_peers_sim(&self, a: &SimulationIdentity, b: &SimulationIdentity) -> Vec<SimulationIdentity>;
}

impl MeshBridge for Mesh {
    fn online_peers_sim(&self) -> Vec<SimulationIdentity> {
        self.peers
            .iter()
            .filter(|(_, state)| state.online)
            .map(|(id, _)| (*id).into())
            .collect()
    }

    fn is_online_sim(&self, peer: &SimulationIdentity) -> bool {
        <Self as NetworkTopology<SimulationIdentity>>::is_online(self, peer)
    }

    fn neighbors_sim(&self, peer: &SimulationIdentity) -> Vec<SimulationIdentity> {
        <Self as NetworkTopology<SimulationIdentity>>::neighbors(self, peer)
    }

    fn mutual_peers_sim(&self, a: &SimulationIdentity, b: &SimulationIdentity) -> Vec<SimulationIdentity> {
        <Self as NetworkTopology<SimulationIdentity>>::mutual_peers(self, a, b)
    }
}

/// Type alias for using the real router with simulation topology
pub type SimulationRouter = indras_routing::StoreForwardRouter<
    SimulationIdentity,
    Mesh,
    indras_storage::InMemoryPacketStore<SimulationIdentity>,
>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::topology::MeshBuilder;
    use indras_core::NetworkTopology;
    use std::collections::HashSet;

    #[test]
    fn test_peer_id_conversion() {
        let peer_id = PeerId('A');
        let sim_id: SimulationIdentity = peer_id.into();
        assert_eq!(sim_id.0, 'A');

        let back: PeerId = sim_id.into();
        assert_eq!(back.0, 'A');
    }

    #[test]
    fn test_mesh_as_topology() {
        // Create a line mesh with 3 peers: A - B - C
        let mesh = MeshBuilder::new(3).line();

        let a = SimulationIdentity::new('A').unwrap();
        let b = SimulationIdentity::new('B').unwrap();
        let c = SimulationIdentity::new('C').unwrap();

        // Use trait methods via the NetworkTopology trait
        let topology: &dyn NetworkTopology<SimulationIdentity> = &mesh;

        // Test connectivity
        assert!(topology.are_connected(&a, &b));
        assert!(topology.are_connected(&b, &c));
        assert!(!topology.are_connected(&a, &c)); // Line: A—B—C, no direct A—C

        // Test neighbors
        let a_neighbors = topology.neighbors(&a);
        assert_eq!(a_neighbors.len(), 1);
        assert!(a_neighbors.contains(&b));

        let b_neighbors = topology.neighbors(&b);
        assert_eq!(b_neighbors.len(), 2);
        assert!(b_neighbors.contains(&a));
        assert!(b_neighbors.contains(&c));
    }

    #[test]
    fn test_mesh_mutual_peers() {
        // Create a diamond: A connects to B and C, both B and C connect to D
        //     A
        //    / \
        //   B   C
        //    \ /
        //     D
        let mesh = crate::from_edges(&[('A', 'B'), ('A', 'C'), ('B', 'D'), ('C', 'D')]);

        let a = SimulationIdentity::new('A').unwrap();
        let d = SimulationIdentity::new('D').unwrap();
        let b_id = SimulationIdentity::new('B').unwrap();
        let c_id = SimulationIdentity::new('C').unwrap();

        // Use trait methods
        let topology: &dyn NetworkTopology<SimulationIdentity> = &mesh;

        // Mutual peers of A and D should be B and C (they're both neighbors of A and D)
        let mutuals = topology.mutual_peers(&a, &d);
        assert_eq!(mutuals.len(), 2);
        assert!(mutuals.contains(&b_id));
        assert!(mutuals.contains(&c_id));
    }

    #[test]
    fn test_online_status() {
        let mut mesh = MeshBuilder::new(3).line();

        let a = SimulationIdentity::new('A').unwrap();
        let b = SimulationIdentity::new('B').unwrap();

        // Initially all peers are offline
        assert!(!mesh.is_online(&a));
        assert!(!mesh.is_online(&b));

        // Set peer A online
        mesh.peers.get_mut(&PeerId('A')).unwrap().online = true;

        // Now A is online, B is still offline
        let topology: &dyn NetworkTopology<SimulationIdentity> = &mesh;
        assert!(topology.is_online(&a));
        assert!(!topology.is_online(&b));
    }

    #[test]
    fn test_all_peers() {
        let mesh = MeshBuilder::new(3).line();

        let topology: &dyn NetworkTopology<SimulationIdentity> = &mesh;
        let all: HashSet<_> = topology.peers().into_iter().collect();

        assert_eq!(all.len(), 3);
        assert!(all.contains(&SimulationIdentity::new('A').unwrap()));
        assert!(all.contains(&SimulationIdentity::new('B').unwrap()));
        assert!(all.contains(&SimulationIdentity::new('C').unwrap()));
    }

    #[test]
    fn test_mesh_bridge_helper_trait() {
        let mut mesh = MeshBuilder::new(3).line();

        let a = SimulationIdentity::new('A').unwrap();
        let b = SimulationIdentity::new('B').unwrap();

        // Test online_peers_sim
        assert_eq!(mesh.online_peers_sim().len(), 0);

        mesh.peers.get_mut(&PeerId('A')).unwrap().online = true;
        mesh.peers.get_mut(&PeerId('B')).unwrap().online = true;

        let online = mesh.online_peers_sim();
        assert_eq!(online.len(), 2);
        assert!(online.contains(&a));
        assert!(online.contains(&b));

        // Test is_online_sim
        assert!(mesh.is_online_sim(&a));
        assert!(!mesh.is_online_sim(&SimulationIdentity::new('C').unwrap()));

        // Test neighbors_sim
        let neighbors = mesh.neighbors_sim(&b);
        assert_eq!(neighbors.len(), 2);
    }
}
