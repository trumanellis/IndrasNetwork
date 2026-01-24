//! Pre-defined simulation scenarios for Indra's Network
//!
//! Includes the canonical A-B-C example and other test cases

use tracing::{debug, info};

use crate::simulation::{SimConfig, Simulation};
use crate::topology::from_edges;
use crate::types::PeerId;

/// Run the canonical A-B-C scenario from the spec:
///
/// ```text
/// A wakes up
/// B wakes up
/// B messages A awake
/// A needs to send a message to C but C is asleep
/// A and C know the set of their mutual peers so A passes the message on to B, sealed and addressed to C
/// A goes to sleep
/// C wakes and messages B awake
/// B sees a packet addressed to C and passes it on
/// B then backpropagates a message for A, next time they connect: message received
/// B deletes their copy of the packet
/// ```
pub fn run_abc_scenario() -> Simulation {
    info!("=== Running A-B-C Scenario ===");

    // Create triangle topology: A - B - C with A-C connected through B
    // Actually, for the mutual peer scenario, A and C should both know B
    // Let's use: A-B, B-C, A-C (triangle)
    let mesh = from_edges(&[('A', 'B'), ('B', 'C'), ('A', 'C')]);

    info!(topology = %mesh.visualize(), "Mesh topology created");

    let mut sim = Simulation::new(
        mesh,
        SimConfig {
            wake_probability: 0.0,  // Manual control
            sleep_probability: 0.0, // Manual control
            trace_routing: true,
            ..Default::default()
        },
    );

    // Initial state: all offline
    info!("Initial state: all peers offline");

    // Step 1: A wakes up
    info!(step = 1, action = "A wakes up");
    sim.force_online(PeerId('A'));
    sim.step();
    debug!(state = %sim.state_summary(), "After step 1");

    // Step 2: B wakes up (B messages A awake)
    info!(step = 2, action = "B wakes up, signals A");
    sim.force_online(PeerId('B'));
    sim.step();
    debug!(state = %sim.state_summary(), "After step 2");

    // Step 3: A needs to send message to C but C is asleep
    info!(
        step = 3,
        action = "A sends message to C (C is offline)",
        note = "A and C have mutual peer B, so A passes message to B"
    );
    sim.send_message(PeerId('A'), PeerId('C'), b"Hello from A to C!".to_vec());
    sim.step();
    debug!(state = %sim.state_summary(), "After step 3");

    // Step 4: A goes to sleep
    info!(step = 4, action = "A goes to sleep");
    sim.force_offline(PeerId('A'));
    sim.step();
    debug!(state = %sim.state_summary(), "After step 4");

    // Step 5: C wakes and messages B awake
    info!(step = 5, action = "C wakes up, signals B");
    sim.force_online(PeerId('C'));
    sim.step();
    debug!(state = %sim.state_summary(), "After step 5");

    // Step 6: B sees packet addressed to C and passes it on
    info!(step = 6, action = "B delivers packet to C");
    sim.step();
    debug!(state = %sim.state_summary(), "After step 6");

    // Step 7: Back-propagation begins
    info!(step = 7, action = "Back-propagation (A offline, waiting)");
    sim.step();
    debug!(state = %sim.state_summary(), "After step 7");

    // Step 8: A comes back online, back-prop completes
    info!(step = 8, action = "A comes online, back-prop completes");
    sim.force_online(PeerId('A'));
    sim.run_ticks(3);
    debug!(state = %sim.state_summary(), "After step 8");

    // Log final statistics
    info!(
        messages_sent = sim.stats.messages_sent,
        messages_delivered = sim.stats.messages_delivered,
        direct_deliveries = sim.stats.direct_deliveries,
        relayed_deliveries = sim.stats.relayed_deliveries,
        backprops_completed = sim.stats.backprops_completed,
        total_hops = sim.stats.total_hops,
        "Final statistics"
    );

    // Log event log
    for event in &sim.event_log {
        debug!(event = ?event, "Event log entry");
    }

    sim
}

/// Scenario: Multi-hop relay through a line topology
/// A - B - C - D - E
/// A sends to E when only A and B are online
pub fn run_line_relay_scenario() -> Simulation {
    info!("=== Running Line Relay Scenario ===");

    let mesh = from_edges(&[('A', 'B'), ('B', 'C'), ('C', 'D'), ('D', 'E')]);

    info!(topology = %mesh.visualize(), "Mesh topology created");

    let mut sim = Simulation::new(
        mesh,
        SimConfig {
            wake_probability: 0.0,
            sleep_probability: 0.0,
            trace_routing: true,
            ..Default::default()
        },
    );

    // Start with A, B online
    sim.force_online(PeerId('A'));
    sim.force_online(PeerId('B'));

    info!(action = "A sends to E (only A, B online)");
    sim.send_message(PeerId('A'), PeerId('E'), b"Multi-hop test".to_vec());
    sim.run_ticks(3);
    debug!(state = %sim.state_summary(), "After initial send");

    // Wake peers one by one
    for peer in ['C', 'D', 'E'] {
        info!(peer = %peer, action = "comes online");
        sim.force_online(PeerId(peer));
        sim.run_ticks(3);
        debug!(state = %sim.state_summary(), "After {} online", peer);
    }

    info!(
        messages_delivered = sim.stats.messages_delivered,
        relayed_deliveries = sim.stats.relayed_deliveries,
        total_hops = sim.stats.total_hops,
        "Final statistics"
    );

    sim
}

/// Scenario: Gossip-style broadcast to multiple destinations
/// Hub-and-spoke: A in center sends to all others
pub fn run_broadcast_scenario() -> Simulation {
    info!("=== Running Broadcast Scenario ===");

    use crate::topology::MeshBuilder;
    let mesh = MeshBuilder::new(5).star();

    info!(topology = %mesh.visualize(), "Mesh topology created");

    let mut sim = Simulation::new(
        mesh,
        SimConfig {
            wake_probability: 0.0,
            sleep_probability: 0.0,
            trace_routing: true,
            ..Default::default()
        },
    );

    // All peers online
    for c in 'A'..='E' {
        sim.force_online(PeerId(c));
    }

    // A broadcasts to all
    info!(action = "A broadcasts to B, C, D, E");
    for dest in ['B', 'C', 'D', 'E'] {
        sim.send_message(
            PeerId('A'),
            PeerId(dest),
            format!("Broadcast to {}", dest).into_bytes(),
        );
    }

    sim.run_ticks(5);

    info!(
        messages_sent = sim.stats.messages_sent,
        messages_delivered = sim.stats.messages_delivered,
        direct_deliveries = sim.stats.direct_deliveries,
        "Final statistics"
    );

    sim
}

/// Scenario: Random network with probabilistic online/offline transitions
pub fn run_random_chaos_scenario(ticks: u64) -> Simulation {
    info!(ticks = ticks, "=== Running Random Chaos Scenario ===");

    use crate::topology::MeshBuilder;
    let mesh = MeshBuilder::new(8).random(0.4);

    info!(topology = %mesh.visualize(), "Mesh topology created");

    let mut sim = Simulation::new(
        mesh,
        SimConfig {
            wake_probability: 0.3,
            sleep_probability: 0.2,
            initial_online_probability: 0.5,
            max_ticks: ticks,
            trace_routing: false, // Less verbose for chaos
            ..Default::default()
        },
    );

    // Initialize and start
    sim.initialize();

    // Queue some initial messages
    sim.send_message(PeerId('A'), PeerId('H'), b"Cross-network test".to_vec());
    sim.send_message(PeerId('B'), PeerId('G'), b"Another test".to_vec());
    sim.send_message(PeerId('C'), PeerId('F'), b"Third test".to_vec());

    // Run simulation
    while sim.tick < ticks {
        sim.step();

        // Inject occasional new messages
        if sim.tick.is_multiple_of(10) {
            let from = PeerId((b'A' + (sim.tick % 8) as u8) as char);
            let to = PeerId((b'H' - (sim.tick % 8) as u8) as char);
            if from != to {
                sim.send_message(
                    from,
                    to,
                    format!("Message at tick {}", sim.tick).into_bytes(),
                );
            }
        }

        if sim.tick.is_multiple_of(20) {
            debug!(tick = sim.tick, state = %sim.state_summary(), "Progress");
        }
    }

    let delivery_rate = if sim.stats.messages_sent > 0 {
        sim.stats.messages_delivered as f64 / sim.stats.messages_sent as f64 * 100.0
    } else {
        0.0
    };

    info!(
        messages_sent = sim.stats.messages_sent,
        messages_delivered = sim.stats.messages_delivered,
        messages_dropped = sim.stats.messages_dropped,
        direct_deliveries = sim.stats.direct_deliveries,
        relayed_deliveries = sim.stats.relayed_deliveries,
        backprops_completed = sim.stats.backprops_completed,
        wake_events = sim.stats.wake_events,
        sleep_events = sim.stats.sleep_events,
        delivery_rate = format!("{:.1}%", delivery_rate),
        "Final statistics"
    );

    sim
}

/// Scenario: Partition and reconnect
/// Network splits into two halves, then reconnects
pub fn run_partition_scenario() -> Simulation {
    info!("=== Running Partition Scenario ===");

    // Create two clusters connected by a single bridge node
    // Cluster 1: A, B, C (fully connected)
    // Cluster 2: D, E, F (fully connected)
    // Bridge: C - D
    let mesh = from_edges(&[
        // Cluster 1
        ('A', 'B'),
        ('A', 'C'),
        ('B', 'C'),
        // Cluster 2
        ('D', 'E'),
        ('D', 'F'),
        ('E', 'F'),
        // Bridge
        ('C', 'D'),
    ]);

    info!(topology = %mesh.visualize(), "Mesh topology created");

    let mut sim = Simulation::new(
        mesh,
        SimConfig {
            wake_probability: 0.0,
            sleep_probability: 0.0,
            trace_routing: true,
            ..Default::default()
        },
    );

    // All online initially
    for c in 'A'..='F' {
        sim.force_online(PeerId(c));
    }

    info!(action = "A sends to F (full connectivity)");
    sim.send_message(PeerId('A'), PeerId('F'), b"Cross-cluster message".to_vec());
    sim.run_ticks(5);
    debug!(delivered = sim.stats.messages_delivered, "After first send");

    info!(action = "Bridge nodes C and D go offline (partition)");
    sim.force_offline(PeerId('C'));
    sim.force_offline(PeerId('D'));

    info!(action = "A tries to send to F (partitioned)");
    sim.send_message(
        PeerId('A'),
        PeerId('F'),
        b"Message during partition".to_vec(),
    );
    sim.run_ticks(5);
    debug!(
        delivered = sim.stats.messages_delivered,
        note = "Should be held, not delivered",
        "After partition send"
    );

    info!(action = "Bridge reconnects (C, D online)");
    sim.force_online(PeerId('C'));
    sim.force_online(PeerId('D'));
    sim.run_ticks(10);

    info!(
        messages_sent = sim.stats.messages_sent,
        messages_delivered = sim.stats.messages_delivered,
        messages_dropped = sim.stats.messages_dropped,
        "Final statistics"
    );

    sim
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_abc_scenario_delivers() {
        let sim = run_abc_scenario();
        assert_eq!(sim.stats.messages_delivered, 1);
    }

    #[test]
    fn test_broadcast_delivers_all() {
        let sim = run_broadcast_scenario();
        assert_eq!(sim.stats.messages_delivered, 4);
        assert_eq!(sim.stats.direct_deliveries, 4);
    }

    #[test]
    fn test_line_relay_delivers() {
        let sim = run_line_relay_scenario();
        assert_eq!(sim.stats.messages_delivered, 1);
        assert!(sim.stats.relayed_deliveries > 0);
    }
}
