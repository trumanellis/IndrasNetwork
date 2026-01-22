//! Simulation engine for Indra's Network
//!
//! Implements discrete-time simulation with:
//! - Random online/offline state transitions
//! - Awake signals and update requests
//! - Store-and-forward routing for offline peers
//! - Back-propagation of delivery confirmations

use std::collections::{BTreeSet, VecDeque};
use rand::Rng;
use tracing::{debug, info, trace, warn};

use crate::types::*;
use crate::topology::Mesh;

/// Configuration for the simulation
#[derive(Debug, Clone)]
pub struct SimConfig {
    /// Probability a peer comes online each tick (if offline)
    pub wake_probability: f64,
    /// Probability a peer goes offline each tick (if online)
    pub sleep_probability: f64,
    /// Maximum simulation ticks
    pub max_ticks: u64,
    /// Initial online probability for each peer
    pub initial_online_probability: f64,
    /// Enable detailed tracing of packet routing
    pub trace_routing: bool,
    /// Maximum ticks a message can wait in pending_sends before expiring (None = no limit)
    pub message_timeout: Option<u64>,
    /// Maximum ticks a back-propagation can stall before being abandoned (None = no limit)
    pub backprop_timeout: Option<u64>,
    /// Maximum retry attempts for offline sender before dropping message
    pub max_sender_retries: Option<u32>,
}

impl Default for SimConfig {
    fn default() -> Self {
        Self {
            wake_probability: 0.3,
            sleep_probability: 0.2,
            max_ticks: 100,
            initial_online_probability: 0.5,
            trace_routing: true,
            message_timeout: Some(50),
            backprop_timeout: Some(100),
            max_sender_retries: Some(10),
        }
    }
}

/// The simulation state
#[derive(Debug)]
pub struct Simulation {
    /// The network mesh
    pub mesh: Mesh,
    /// Current simulation tick
    pub tick: u64,
    /// Configuration
    pub config: SimConfig,
    /// Global event log (all events)
    pub event_log: Vec<NetworkEvent>,
    /// Pending messages to be routed
    pending_sends: VecDeque<PendingSend>,
    /// Active back-propagations
    backprops: Vec<BackPropState>,
    /// Statistics
    pub stats: SimStats,
}

#[derive(Debug, Clone)]
struct PendingSend {
    from: PeerId,
    to: PeerId,
    payload: Vec<u8>,
    created_tick: u64,
    /// Number of times we've retried due to offline sender
    retry_count: u32,
}

#[derive(Debug, Clone)]
struct BackPropState {
    packet_id: PacketId,
    /// Path the packet took (for back-propagation)
    path: Vec<PeerId>,
    /// Current position in back-propagation (index in path, going backwards)
    backprop_index: usize,
    delivered_tick: u64,
}

/// Simulation statistics
#[derive(Debug, Clone, Default)]
pub struct SimStats {
    pub messages_sent: u64,
    pub messages_delivered: u64,
    pub messages_dropped: u64,
    pub messages_expired: u64,
    pub total_hops: u64,
    pub direct_deliveries: u64,
    pub relayed_deliveries: u64,
    pub backprops_completed: u64,
    pub backprops_timed_out: u64,
    pub wake_events: u64,
    pub sleep_events: u64,
    /// Total delivery latency (ticks from send to delivery)
    pub total_delivery_latency: u64,
    /// Total backprop latency (ticks from delivery to confirmation complete)
    pub total_backprop_latency: u64,
}

impl Simulation {
    /// Create a new simulation with the given mesh and configuration
    pub fn new(mesh: Mesh, config: SimConfig) -> Self {
        Self {
            mesh,
            tick: 0,
            config,
            event_log: Vec::new(),
            pending_sends: VecDeque::new(),
            backprops: Vec::new(),
            stats: SimStats::default(),
        }
    }

    /// Initialize the simulation - set initial online/offline states
    pub fn initialize(&mut self) {
        let mut rng = rand::rng();
        let peer_ids: Vec<PeerId> = self.mesh.peer_ids();

        for peer_id in peer_ids {
            let online = rng.random::<f64>() < self.config.initial_online_probability;
            if let Some(peer) = self.mesh.peers.get_mut(&peer_id) {
                peer.online = online;
                if online {
                    peer.last_online_tick = Some(0);
                    self.emit_event(NetworkEvent::Awake { peer: peer_id, tick: 0 });
                }
            }
        }
        info!("Simulation initialized at tick 0");
    }

    /// Queue a message to be sent
    pub fn send_message(&mut self, from: PeerId, to: PeerId, payload: Vec<u8>) {
        self.pending_sends.push_back(PendingSend {
            from,
            to,
            payload,
            created_tick: self.tick,
            retry_count: 0,
        });
        self.stats.messages_sent += 1;
    }

    /// Run a single simulation tick
    pub fn step(&mut self) {
        self.tick += 1;
        trace!("=== Tick {} ===", self.tick);

        // 1. Process wake/sleep transitions
        self.process_state_transitions();

        // 2. Process awake signals - peers coming online request updates
        self.process_awake_signals();

        // 3. Route pending messages
        self.process_pending_sends();

        // 4. Process relay queues for online peers
        self.process_relay_queues();

        // 5. Process back-propagations
        self.process_backprops();
    }

    /// Run simulation until max_ticks or no more activity
    pub fn run(&mut self) {
        self.initialize();
        
        while self.tick < self.config.max_ticks {
            self.step();
        }

        info!("Simulation complete at tick {}", self.tick);
        info!("Stats: {:?}", self.stats);
    }

    /// Run for a specific number of ticks
    pub fn run_ticks(&mut self, ticks: u64) {
        for _ in 0..ticks {
            self.step();
        }
    }

    fn process_state_transitions(&mut self) {
        let mut rng = rand::rng();
        let peer_ids: Vec<PeerId> = self.mesh.peer_ids();

        for peer_id in peer_ids {
            let (was_online, should_transition) = {
                let peer = self.mesh.peers.get(&peer_id).unwrap();
                let prob = if peer.online {
                    self.config.sleep_probability
                } else {
                    self.config.wake_probability
                };
                (peer.online, rng.random::<f64>() < prob)
            };

            if should_transition {
                let peer = self.mesh.peers.get_mut(&peer_id).unwrap();
                peer.online = !was_online;

                if peer.online {
                    // Waking up
                    peer.last_online_tick = Some(self.tick);
                    self.emit_event(NetworkEvent::Awake { peer: peer_id, tick: self.tick });
                    self.stats.wake_events += 1;
                    debug!("Peer {} woke up at tick {}", peer_id, self.tick);
                } else {
                    // Going to sleep
                    self.emit_event(NetworkEvent::Sleep { peer: peer_id, tick: self.tick });
                    self.stats.sleep_events += 1;
                    debug!("Peer {} went to sleep at tick {}", peer_id, self.tick);
                }
            }
        }
    }

    fn process_awake_signals(&mut self) {
        // When a peer wakes up, it signals all connected peers to send any pending packets
        let peer_ids: Vec<PeerId> = self.mesh.peer_ids();

        for peer_id in peer_ids {
            let (just_woke, neighbors) = {
                let peer = self.mesh.peers.get(&peer_id).unwrap();
                let just_woke = peer.online 
                    && peer.last_online_tick == Some(self.tick);
                let neighbors: Vec<PeerId> = peer.connections.iter().copied().collect();
                (just_woke, neighbors)
            };

            if just_woke {
                trace!("Peer {} broadcasting awake signal to {:?}", peer_id, neighbors);
                // Check each neighbor's relay queue for packets addressed to us
                for neighbor_id in neighbors {
                    self.deliver_pending_packets(neighbor_id, peer_id);
                }
            }
        }
    }

    /// Deliver any packets from `from` that are addressed to `to`
    fn deliver_pending_packets(&mut self, from: PeerId, to: PeerId) {
        let packets_to_deliver: Vec<SealedPacket> = {
            let from_peer = self.mesh.peers.get(&from).unwrap();
            from_peer.relay_queue
                .iter()
                .filter(|p| p.destination == to)
                .cloned()
                .collect()
        };

        for packet in packets_to_deliver {
            let packet_id = packet.id;
            
            // Remove from relay queue
            {
                let from_peer = self.mesh.peers.get_mut(&from).unwrap();
                from_peer.relay_queue.retain(|p| p.id != packet_id);
            }

            // Deliver to destination
            self.deliver_packet(packet, from);
        }
    }

    fn deliver_packet(&mut self, packet: SealedPacket, via: PeerId) {
        let dest_id = packet.destination;
        let packet_id = packet.id;
        let created_at = packet.created_at;

        // Add to destination's inbox and mark as delivered
        {
            let dest_peer = self.mesh.peers.get_mut(&dest_id).unwrap();
            dest_peer.inbox.push(packet.clone());
            dest_peer.delivered.push(packet_id);
        }

        self.emit_event(NetworkEvent::Delivered {
            packet_id,
            to: dest_id,
            tick: self.tick,
        });

        // Track delivery latency
        let delivery_latency = self.tick.saturating_sub(created_at);
        self.stats.total_delivery_latency += delivery_latency;

        info!("Packet {} delivered to {} via {} at tick {} (latency: {} ticks)",
              packet_id, dest_id, via, self.tick, delivery_latency);
        self.stats.messages_delivered += 1;
        self.stats.total_hops += packet.visited.len() as u64;

        if packet.visited.len() == 1 {
            self.stats.direct_deliveries += 1;
        } else {
            self.stats.relayed_deliveries += 1;
        }

        // Start back-propagation
        let path: Vec<PeerId> = packet.visited.iter().copied().collect();
        if path.len() > 1 {
            self.backprops.push(BackPropState {
                packet_id,
                path: path.clone(),
                backprop_index: 0,
                delivered_tick: self.tick,
            });

            // Store packet_id in path for relay queue cleanup later
            // The path tells us which peers are holding this packet
        }
    }

    fn process_pending_sends(&mut self) {
        let sends: Vec<PendingSend> = self.pending_sends.drain(..).collect();

        for send in sends {
            // Check for message expiration
            if let Some(timeout) = self.config.message_timeout
                && self.tick - send.created_tick > timeout {
                    debug!("Message from {} to {} expired after {} ticks",
                           send.from, send.to, self.tick - send.created_tick);
                    self.stats.messages_expired += 1;
                    self.stats.messages_dropped += 1;
                    // Create a placeholder packet_id for the dropped event
                    let packet_id = PacketId { source: send.from, sequence: 0 };
                    self.emit_event(NetworkEvent::Dropped {
                        packet_id,
                        reason: DropReason::Expired,
                        tick: self.tick,
                    });
                    continue;
                }

            self.emit_event(NetworkEvent::Send {
                from: send.from,
                to: send.to,
                payload: send.payload.clone(),
                tick: self.tick,
            });

            self.route_message_with_retry(send.from, send.to, send.payload, send.created_tick, send.retry_count);
        }
    }

    fn route_message_with_retry(&mut self, from: PeerId, to: PeerId, payload: Vec<u8>, created_tick: u64, retry_count: u32) {
        // Check if sender is online
        let from_online = self.mesh.peers.get(&from).map(|p| p.online).unwrap_or(false);
        if !from_online {
            // Check retry limit
            if let Some(max_retries) = self.config.max_sender_retries
                && retry_count >= max_retries {
                    warn!("Message from {} to {} dropped: sender offline after {} retries",
                          from, to, retry_count);
                    self.stats.messages_dropped += 1;
                    let packet_id = PacketId { source: from, sequence: 0 };
                    self.emit_event(NetworkEvent::Dropped {
                        packet_id,
                        reason: DropReason::SenderOffline,
                        tick: self.tick,
                    });
                    return;
                }
            debug!("Sender {} is offline, queueing message (retry {})", from, retry_count + 1);
            self.pending_sends.push_back(PendingSend {
                from, to, payload, created_tick, retry_count: retry_count + 1,
            });
            return;
        }

        // Create packet
        let packet_id = {
            let from_peer = self.mesh.peers.get_mut(&from).unwrap();
            from_peer.next_packet_id()
        };

        // Calculate routing hints (mutual peers)
        let routing_hints = self.mesh.mutual_peers(from, to);

        let packet = SealedPacket::new(
            packet_id,
            from,
            to,
            payload,
            routing_hints,
            self.tick,
        );

        // Check if destination is online and directly reachable
        let dest_online = self.mesh.peers.get(&to).map(|p| p.online).unwrap_or(false);
        let direct_connection = self.mesh.are_connected(from, to);

        if direct_connection && dest_online {
            // Direct delivery
            debug!("Direct delivery: {} -> {}", from, to);
            self.deliver_packet(packet, from);
        } else if direct_connection && !dest_online {
            // Destination offline but directly connected - hold for later
            debug!("Destination {} offline, holding packet at {}", to, from);
            let from_peer = self.mesh.peers.get_mut(&from).unwrap();
            from_peer.relay_queue.push(packet);
        } else {
            // Need to route through intermediary
            self.find_relay_path(packet, from);
        }
    }

    fn find_relay_path(&mut self, mut packet: SealedPacket, current: PeerId) {
        let dest = packet.destination;

        // Get online neighbors who haven't seen this packet
        let candidates: Vec<PeerId> = {
            let neighbors = self.mesh.neighbors(current).cloned().unwrap_or_default();
            neighbors.into_iter()
                .filter(|n| !packet.was_visited(*n))
                .filter(|n| self.mesh.peers.get(n).map(|p| p.online).unwrap_or(false))
                .collect()
        };

        // Priority 1: Use routing_hints (mutual peers who can reach destination)
        // These were pre-computed when the packet was created
        let hint_relay = candidates.iter()
            .find(|c| packet.routing_hints.contains(c))
            .copied();

        // Priority 2: Neighbors who are connected to destination
        let dest_neighbors: BTreeSet<PeerId> = self.mesh.neighbors(dest)
            .cloned()
            .unwrap_or_default();

        let best_relay = hint_relay
            .or_else(|| candidates.iter().find(|c| dest_neighbors.contains(c)).copied())
            .or_else(|| candidates.first().copied());

        match best_relay {
            Some(relay) => {
                debug!("Routing packet {} through relay {}", packet.id, relay);
                
                if !packet.decrement_ttl() {
                    self.emit_event(NetworkEvent::Dropped {
                        packet_id: packet.id,
                        reason: DropReason::TtlExpired,
                        tick: self.tick,
                    });
                    self.stats.messages_dropped += 1;
                    return;
                }

                packet.mark_visited(relay);

                self.emit_event(NetworkEvent::Relay {
                    from: current,
                    via: relay,
                    to: dest,
                    packet_id: packet.id,
                    tick: self.tick,
                });

                // Add to relay peer's queue
                let relay_peer = self.mesh.peers.get_mut(&relay).unwrap();
                relay_peer.relay_queue.push(packet);
            }
            None => {
                // No online relay available - store at current peer for later
                if candidates.is_empty() {
                    debug!("No relay available for packet {}, holding at {}", packet.id, current);
                    let current_peer = self.mesh.peers.get_mut(&current).unwrap();
                    current_peer.relay_queue.push(packet);
                } else {
                    warn!("No path to {} from {}", dest, current);
                    self.emit_event(NetworkEvent::Dropped {
                        packet_id: packet.id,
                        reason: DropReason::NoRoute,
                        tick: self.tick,
                    });
                    self.stats.messages_dropped += 1;
                }
            }
        }
    }

    fn process_relay_queues(&mut self) {
        let peer_ids: Vec<PeerId> = self.mesh.peer_ids();

        for peer_id in peer_ids {
            let (online, packets_to_forward) = {
                let peer = self.mesh.peers.get(&peer_id).unwrap();
                if !peer.online {
                    continue;
                }

                // Get packets that might be deliverable now
                let packets: Vec<SealedPacket> = peer.relay_queue.clone();
                (peer.online, packets)
            };

            if !online {
                continue;
            }

            for packet in packets_to_forward {
                let dest = packet.destination;
                let dest_online = self.mesh.peers.get(&dest).map(|p| p.online).unwrap_or(false);
                let direct = self.mesh.are_connected(peer_id, dest);

                if direct && dest_online {
                    // Can deliver now - destination is directly connected and online
                    {
                        let peer = self.mesh.peers.get_mut(&peer_id).unwrap();
                        peer.relay_queue.retain(|p| p.id != packet.id);
                    }
                    self.deliver_packet(packet, peer_id);
                } else if !direct {
                    // Not directly connected to destination - try to forward to next hop
                    // Remove from current queue and try to route further
                    {
                        let peer = self.mesh.peers.get_mut(&peer_id).unwrap();
                        peer.relay_queue.retain(|p| p.id != packet.id);
                    }
                    self.find_relay_path(packet, peer_id);
                }
                // If directly connected but destination offline, packet stays in queue
            }
        }
    }

    fn process_backprops(&mut self) {
        let mut completed_indices = Vec::new();
        let mut timed_out_indices = Vec::new();
        let mut events_to_emit = Vec::new();

        for (idx, backprop) in self.backprops.iter_mut().enumerate() {
            // Check for backprop timeout
            if let Some(timeout) = self.config.backprop_timeout
                && self.tick - backprop.delivered_tick > timeout {
                    timed_out_indices.push(idx);
                    continue;
                }

            // Work backwards through the path
            if backprop.backprop_index >= backprop.path.len() - 1 {
                completed_indices.push(idx);
                continue;
            }

            let current_idx = backprop.path.len() - 1 - backprop.backprop_index;
            let next_idx = current_idx - 1;

            let current = backprop.path[current_idx];
            let next = backprop.path[next_idx];

            // Check if both peers are online
            let current_online = self.mesh.peers.get(&current).map(|p| p.online).unwrap_or(false);
            let next_online = self.mesh.peers.get(&next).map(|p| p.online).unwrap_or(false);

            if current_online && next_online {
                events_to_emit.push((backprop.packet_id, current, next, self.tick));
                backprop.backprop_index += 1;
                debug!("Back-prop {} at {} -> {}", backprop.packet_id, current, next);
            }
        }

        // Emit events after the mutable borrow ends
        for (packet_id, current, next, tick) in events_to_emit {
            self.emit_event(NetworkEvent::BackProp {
                packet_id,
                from: current,
                via: current,
                to: next,
                tick,
            });
        }

        // Remove timed out back-propagations (process first to avoid index shift issues)
        for idx in timed_out_indices.into_iter().rev() {
            let bp = self.backprops.remove(idx);
            self.stats.backprops_timed_out += 1;
            warn!("Back-propagation timed out for packet {} after {} ticks",
                  bp.packet_id, self.tick - bp.delivered_tick);
        }

        // Remove completed back-propagations and clean up relay queues
        for idx in completed_indices.into_iter().rev() {
            let bp = self.backprops.remove(idx);

            // Track backprop latency
            let backprop_latency = self.tick.saturating_sub(bp.delivered_tick);
            self.stats.total_backprop_latency += backprop_latency;

            // Clean up relay queues at intermediate peers (not source or destination)
            // The path is [source, relay1, relay2, ..., destination]
            if bp.path.len() > 2 {
                for relay_id in &bp.path[1..bp.path.len()-1] {
                    if let Some(peer) = self.mesh.peers.get_mut(relay_id) {
                        let before_len = peer.relay_queue.len();
                        peer.relay_queue.retain(|p| p.id != bp.packet_id);
                        let removed = before_len - peer.relay_queue.len();
                        if removed > 0 {
                            debug!("Cleaned up {} packet(s) from {}'s relay queue after backprop",
                                   removed, relay_id);
                        }
                    }
                }
            }

            self.stats.backprops_completed += 1;
            info!("Back-propagation complete for packet {} (latency: {} ticks)",
                  bp.packet_id, backprop_latency);
        }
    }

    fn emit_event(&mut self, event: NetworkEvent) {
        if self.config.trace_routing {
            trace!("Event: {:?}", event);
        }
        self.event_log.push(event);
    }

    /// Get a summary of the current state
    pub fn state_summary(&self) -> String {
        let online_count = self.mesh.peers.values().filter(|p| p.online).count();
        let total_relay_queue: usize = self.mesh.peers.values()
            .map(|p| p.relay_queue.len())
            .sum();

        format!(
            "Tick {}: {} online, {} relay queued, {} backprops pending",
            self.tick,
            online_count,
            total_relay_queue,
            self.backprops.len()
        )
    }

    /// Check if a specific peer is online
    pub fn is_online(&self, peer: PeerId) -> bool {
        self.mesh.peers.get(&peer).map(|p| p.online).unwrap_or(false)
    }

    /// Force a peer online
    pub fn force_online(&mut self, peer: PeerId) {
        if let Some(p) = self.mesh.peers.get_mut(&peer)
            && !p.online {
                p.online = true;
                p.last_online_tick = Some(self.tick);
                self.emit_event(NetworkEvent::Awake { peer, tick: self.tick });
            }
    }

    /// Force a peer offline
    pub fn force_offline(&mut self, peer: PeerId) {
        if let Some(p) = self.mesh.peers.get_mut(&peer)
            && p.online {
                p.online = false;
                self.emit_event(NetworkEvent::Sleep { peer, tick: self.tick });
            }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::topology::MeshBuilder;

    #[test]
    fn test_direct_delivery() {
        let mesh = MeshBuilder::new(3).full_mesh();
        let mut sim = Simulation::new(mesh, SimConfig {
            wake_probability: 0.0,
            sleep_probability: 0.0,
            ..Default::default()
        });
        
        // Force all online
        sim.force_online(PeerId('A'));
        sim.force_online(PeerId('B'));
        sim.force_online(PeerId('C'));
        
        sim.send_message(PeerId('A'), PeerId('B'), vec![1, 2, 3]);
        sim.step();

        assert_eq!(sim.stats.messages_delivered, 1);
        assert_eq!(sim.stats.direct_deliveries, 1);
    }

    #[test]
    fn test_relay_delivery() {
        // A - B - C (line)
        let mesh = MeshBuilder::new(3).line();
        let mut sim = Simulation::new(mesh, SimConfig {
            wake_probability: 0.0,
            sleep_probability: 0.0,
            ..Default::default()
        });

        sim.force_online(PeerId('A'));
        sim.force_online(PeerId('B'));
        sim.force_online(PeerId('C'));

        // A wants to send to C (not directly connected)
        sim.send_message(PeerId('A'), PeerId('C'), vec![1, 2, 3]);
        sim.run_ticks(5);

        assert_eq!(sim.stats.messages_delivered, 1);
        assert_eq!(sim.stats.relayed_deliveries, 1);
    }

    #[test]
    fn test_offline_store_and_forward() {
        let mesh = MeshBuilder::new(3).full_mesh();
        let mut sim = Simulation::new(mesh, SimConfig {
            wake_probability: 0.0,
            sleep_probability: 0.0,
            ..Default::default()
        });

        // A and B online, C offline
        sim.force_online(PeerId('A'));
        sim.force_online(PeerId('B'));
        sim.force_offline(PeerId('C'));

        sim.send_message(PeerId('A'), PeerId('C'), vec![1, 2, 3]);
        sim.run_ticks(3);

        // Message should be held, not delivered yet
        assert_eq!(sim.stats.messages_delivered, 0);

        // C comes online
        sim.force_online(PeerId('C'));
        sim.run_ticks(3);

        // Now it should be delivered
        assert_eq!(sim.stats.messages_delivered, 1);
    }
}
