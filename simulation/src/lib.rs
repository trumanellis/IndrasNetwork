//! # Indra's Network
//!
//! A mesh network simulation library with store-and-forward routing for offline peers.
//!
//! ## Overview
//!
//! This library models peer-to-peer signal propagation across a network where peers
//! may go online and offline at any time. Key features:
//!
//! - **Named peers** (A-Z): Each peer has a unique identifier
//! - **Bidirectional connections**: All peer pairings maintain synced interfaces
//! - **Event-driven architecture**: Append-only event logs for each peer interface
//! - **Store-and-forward routing**: Messages to offline peers are held by intermediaries
//! - **Back-propagation**: Delivery confirmations travel back to the source
//!
//! ## Architecture
//!
//! The simulation follows patterns from iroh-examples:
//!
//! - **Types** (`types.rs`): Core data structures (PeerId, SealedPacket, NetworkEvent)
//! - **Topology** (`topology.rs`): Mesh network construction (ring, full, random, etc.)
//! - **Simulation** (`simulation.rs`): Discrete-time simulation engine
//! - **Scenarios** (`scenarios.rs`): Pre-built test scenarios
//!
//! ## Example: A-B-C Scenario
//!
//! ```rust,ignore
//! use indras_simulation::*;
//!
//! // Create a triangle mesh: A - B, B - C, A - C
//! let mesh = from_edges(&[('A', 'B'), ('B', 'C'), ('A', 'C')]);
//!
//! // Create simulation with manual control
//! let mut sim = Simulation::new(mesh, SimConfig {
//!     wake_probability: 0.0,  // Manual control
//!     sleep_probability: 0.0,
//!     ..Default::default()
//! });
//!
//! // A and B come online, C stays offline
//! sim.force_online(PeerId('A'));
//! sim.force_online(PeerId('B'));
//!
//! // A sends to C - will be held at A since C is offline
//! sim.send_message(PeerId('A'), PeerId('C'), b"Hello C!".to_vec());
//! sim.run_ticks(3);
//!
//! // C comes online - message gets delivered
//! sim.force_online(PeerId('C'));
//! sim.run_ticks(3);
//!
//! assert_eq!(sim.stats.messages_delivered, 1);
//! ```
//!
//! ## Peer Interface Model
//!
//! Each pair of connected peers maintains a synced interface consisting of:
//!
//! 1. **Append-only event log**: All events between the peers (CRDT-style, like Automerge)
//! 2. **Pending packets queue**: Sealed packets being held for relay
//!
//! When peers connect, they sync their event logs and exchange pending packets.
//!
//! ## Signal Propagation
//!
//! 1. **Awake signals**: When a peer comes online, it broadcasts to all connections
//! 2. **Update requests**: Neighbors respond with any pending packets
//! 3. **Sealed packets**: Messages to offline peers are sealed (encrypted for destination)
//! 4. **Back-propagation**: Delivery confirmations travel back through the relay path

pub mod types;
pub mod topology;
pub mod simulation;
pub mod scenarios;
pub mod bridge;

#[cfg(test)]
mod integration_scenarios;

// Re-export main types
pub use types::{
    PeerId, 
    PacketId, 
    SealedPacket, 
    NetworkEvent, 
    DropReason,
    PeerState, 
    PeerInterface, 
    EventLog,
    BackPropRecord,
};

pub use topology::{
    Mesh, 
    MeshBuilder, 
    from_edges,
};

pub use simulation::{
    Simulation,
    SimConfig,
    SimStats,
};

pub use bridge::{MeshBridge, SimulationRouter};

// Re-export core types for integration
pub use indras_core::SimulationIdentity;
