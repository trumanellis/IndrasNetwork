//! State types for live instance visualization
//!
//! This module contains types for rendering and controlling the live
//! simulation view, including peer positions, packet animations, and
//! playback state.

use indras_simulation::{NetworkEvent, PacketId, PeerId, Simulation};
use std::collections::HashMap;

/// State for live instance visualization
#[derive(Default)]
pub struct InstanceState {
    /// The simulation being visualized (None if not loaded)
    pub simulation: Option<Simulation>,
    /// Peer positions for rendering (computed by layout algorithm)
    pub peer_positions: HashMap<PeerId, (f64, f64)>,
    /// Active packet animations
    pub packets_in_flight: Vec<PacketAnimation>,
    /// Whether simulation is paused
    pub paused: bool,
    /// Playback speed (ticks per second)
    pub playback_speed: f64,
    /// Recent events for timeline display
    pub recent_events: Vec<NetworkEvent>,
    /// Maximum events to keep in timeline
    pub max_events: usize,
    /// Selected scenario name for this instance
    pub scenario_name: Option<String>,
}

impl InstanceState {
    /// Create a new empty instance state
    pub fn new() -> Self {
        Self {
            simulation: None,
            peer_positions: HashMap::new(),
            packets_in_flight: Vec::new(),
            paused: true,
            playback_speed: 1.0,
            recent_events: Vec::new(),
            max_events: 100,
            scenario_name: None,
        }
    }

    /// Initialize with a simulation
    #[allow(dead_code)] // Reserved for builder pattern
    pub fn with_simulation(mut self, sim: Simulation) -> Self {
        self.simulation = Some(sim);
        self
    }

    /// Get the current tick from simulation
    pub fn current_tick(&self) -> u64 {
        self.simulation.as_ref().map(|s| s.tick).unwrap_or(0)
    }

    /// Get the max ticks from simulation config
    pub fn max_ticks(&self) -> u64 {
        self.simulation
            .as_ref()
            .map(|s| s.config.max_ticks)
            .unwrap_or(0)
    }

    /// Check if a peer is online
    pub fn is_peer_online(&self, peer: PeerId) -> bool {
        self.simulation
            .as_ref()
            .map(|s| s.is_online(peer))
            .unwrap_or(false)
    }

    /// Get relay queue depth for a peer
    pub fn get_queue_depth(&self, peer: PeerId) -> usize {
        self.simulation
            .as_ref()
            .and_then(|s| s.mesh.peers.get(&peer))
            .map(|p| p.relay_queue.len())
            .unwrap_or(0)
    }

    /// Get inbox count for a peer
    pub fn get_inbox_count(&self, peer: PeerId) -> usize {
        self.simulation
            .as_ref()
            .and_then(|s| s.mesh.peers.get(&peer))
            .map(|p| p.inbox.len())
            .unwrap_or(0)
    }

    /// Add an event, maintaining max capacity
    pub fn add_event(&mut self, event: NetworkEvent) {
        self.recent_events.push(event);
        if self.recent_events.len() > self.max_events {
            self.recent_events.remove(0);
        }
    }

    /// Clear all events
    pub fn clear_events(&mut self) {
        self.recent_events.clear();
    }

    /// Compute edges from mesh topology
    pub fn compute_edges(&self) -> Vec<(PeerId, PeerId)> {
        self.simulation
            .as_ref()
            .map(|s| s.mesh.interfaces.keys().copied().collect())
            .unwrap_or_default()
    }

    /// Get all peer IDs
    pub fn peer_ids(&self) -> Vec<PeerId> {
        self.simulation
            .as_ref()
            .map(|s| s.mesh.peer_ids())
            .unwrap_or_default()
    }
}

/// Animation state for a packet moving between peers
#[derive(Debug, Clone)]
pub struct PacketAnimation {
    /// Unique packet identifier
    pub packet_id: PacketId,
    /// Source peer of this hop
    pub from: PeerId,
    /// Destination peer of this hop
    pub to: PeerId,
    /// Animation progress from 0.0 to 1.0
    pub progress: f64,
    /// Tick when animation started
    pub start_tick: u64,
    /// Duration of animation in ticks
    pub duration_ticks: u64,
}

impl PacketAnimation {
    /// Create a new packet animation
    pub fn new(packet_id: PacketId, from: PeerId, to: PeerId, start_tick: u64) -> Self {
        Self {
            packet_id,
            from,
            to,
            progress: 0.0,
            start_tick,
            duration_ticks: 5, // 5 ticks for animation - visible longer
        }
    }

    /// Update progress based on current tick
    pub fn update(&mut self, current_tick: u64) {
        let elapsed = current_tick.saturating_sub(self.start_tick);
        self.progress = (elapsed as f64 / self.duration_ticks as f64).min(1.0);
    }

    /// Check if animation is complete
    pub fn is_complete(&self) -> bool {
        self.progress >= 1.0
    }

    /// Calculate current position given endpoint positions
    pub fn interpolate_position(&self, from_pos: (f64, f64), to_pos: (f64, f64)) -> (f64, f64) {
        let t = self.progress;
        (
            from_pos.0 + (to_pos.0 - from_pos.0) * t,
            from_pos.1 + (to_pos.1 - from_pos.1) * t,
        )
    }
}

/// Visual representation of a peer for rendering
#[allow(dead_code)] // Reserved for future visualization feature
#[derive(Debug, Clone)]
pub struct PeerVisual {
    /// Peer identifier
    pub id: PeerId,
    /// Whether the peer is currently online
    pub online: bool,
    /// Number of packets in relay queue
    pub relay_queue_depth: usize,
    /// Number of packets in inbox
    pub inbox_count: usize,
    /// Screen position (x, y)
    pub position: (f64, f64),
}

#[allow(dead_code)] // Reserved for future visualization feature
impl PeerVisual {
    /// Create from instance state
    pub fn from_state(id: PeerId, state: &InstanceState) -> Self {
        Self {
            id,
            online: state.is_peer_online(id),
            relay_queue_depth: state.get_queue_depth(id),
            inbox_count: state.get_inbox_count(id),
            position: state.peer_positions.get(&id).copied().unwrap_or((0.0, 0.0)),
        }
    }
}

/// Helper to format NetworkEvent for display
pub fn format_network_event(event: &NetworkEvent) -> (String, String, String) {
    match event {
        NetworkEvent::Delivered {
            packet_id,
            to,
            tick,
        } => (
            "delivered".to_string(),
            format!("{}", tick),
            format!("{} delivered to {}", packet_id, to),
        ),
        NetworkEvent::Relay {
            from,
            via,
            to,
            packet_id,
            tick,
        } => (
            "relay".to_string(),
            format!("{}", tick),
            format!("{} relayed {} -> {} -> {}", packet_id, from, via, to),
        ),
        NetworkEvent::Dropped {
            packet_id,
            reason,
            tick,
        } => (
            "dropped".to_string(),
            format!("{}", tick),
            format!("{} dropped: {:?}", packet_id, reason),
        ),
        NetworkEvent::Awake { peer, tick } => (
            "awake".to_string(),
            format!("{}", tick),
            format!("{} came online", peer),
        ),
        NetworkEvent::Sleep { peer, tick } => (
            "sleep".to_string(),
            format!("{}", tick),
            format!("{} went offline", peer),
        ),
        NetworkEvent::Send { from, to, tick, .. } => (
            "send".to_string(),
            format!("{}", tick),
            format!("{} -> {} message sent", from, to),
        ),
        NetworkEvent::BackProp {
            packet_id,
            from,
            via,
            to,
            tick,
        } => (
            "backprop".to_string(),
            format!("{}", tick),
            format!("{} backprop {} -> {} -> {}", packet_id, from, via, to),
        ),
        NetworkEvent::PQSignatureCreated { peer, tick, .. } => (
            "pq".to_string(),
            format!("{}", tick),
            format!("{} created PQ signature", peer),
        ),
        NetworkEvent::PQSignatureVerified {
            peer,
            sender,
            success,
            tick,
            ..
        } => (
            if *success { "pq" } else { "error" }.to_string(),
            format!("{}", tick),
            format!(
                "{} verified {} signature: {}",
                peer,
                sender,
                if *success { "OK" } else { "FAIL" }
            ),
        ),
        NetworkEvent::KEMEncapsulation {
            peer, target, tick, ..
        } => (
            "pq".to_string(),
            format!("{}", tick),
            format!("{} -> {} KEM encapsulation", peer, target),
        ),
        NetworkEvent::KEMDecapsulation {
            peer,
            sender,
            success,
            tick,
            ..
        } => (
            if *success { "pq" } else { "error" }.to_string(),
            format!("{}", tick),
            format!(
                "{} <- {} KEM decapsulation: {}",
                peer,
                sender,
                if *success { "OK" } else { "FAIL" }
            ),
        ),
        NetworkEvent::InviteCreated { from, to, tick, .. } => (
            "invite".to_string(),
            format!("{}", tick),
            format!("{} created invite for {}", from, to),
        ),
        NetworkEvent::InviteAccepted { peer, tick, .. } => (
            "invite".to_string(),
            format!("{}", tick),
            format!("{} accepted invite", peer),
        ),
        NetworkEvent::InviteFailed {
            peer, reason, tick, ..
        } => (
            "error".to_string(),
            format!("{}", tick),
            format!("{} invite failed: {}", peer, reason),
        ),
    }
}
