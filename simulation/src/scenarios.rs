//! Pre-defined simulation scenarios for Indra's Network
//!
//! Includes the canonical A-B-C example and other test cases

use tracing::info;

use crate::topology::from_edges;
use crate::types::PeerId;
use crate::simulation::{Simulation, SimConfig};

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
    let mesh = from_edges(&[
        ('A', 'B'),
        ('B', 'C'),
        ('A', 'C'),
    ]);

    println!("{}", mesh.visualize());

    let mut sim = Simulation::new(mesh, SimConfig {
        wake_probability: 0.0,  // Manual control
        sleep_probability: 0.0, // Manual control  
        trace_routing: true,
        ..Default::default()
    });

    // Initial state: all offline
    println!("\n--- Initial State: All peers offline ---");

    // Step 1: A wakes up
    println!("\n--- Step 1: A wakes up ---");
    sim.force_online(PeerId('A'));
    sim.step();
    println!("  {}", sim.state_summary());

    // Step 2: B wakes up (B messages A awake)
    println!("\n--- Step 2: B wakes up, signals A ---");
    sim.force_online(PeerId('B'));
    sim.step();
    println!("  {}", sim.state_summary());

    // Step 3: A needs to send message to C but C is asleep
    println!("\n--- Step 3: A sends message to C (C is offline) ---");
    println!("  A and C have mutual peer B, so A passes message to B");
    sim.send_message(PeerId('A'), PeerId('C'), b"Hello from A to C!".to_vec());
    sim.step();
    println!("  {}", sim.state_summary());

    // Step 4: A goes to sleep
    println!("\n--- Step 4: A goes to sleep ---");
    sim.force_offline(PeerId('A'));
    sim.step();
    println!("  {}", sim.state_summary());

    // Step 5: C wakes and messages B awake
    println!("\n--- Step 5: C wakes up, signals B ---");
    sim.force_online(PeerId('C'));
    sim.step();
    println!("  {}", sim.state_summary());

    // Step 6: B sees packet addressed to C and passes it on
    println!("\n--- Step 6: B delivers packet to C ---");
    sim.step();
    println!("  {}", sim.state_summary());

    // Step 7: Back-propagation begins
    println!("\n--- Step 7: Back-propagation (A offline, waiting) ---");
    sim.step();
    println!("  {}", sim.state_summary());

    // Step 8: A comes back online, back-prop completes
    println!("\n--- Step 8: A comes online, back-prop completes ---");
    sim.force_online(PeerId('A'));
    sim.run_ticks(3);
    println!("  {}", sim.state_summary());

    // Print final statistics
    println!("\n=== Final Statistics ===");
    println!("  Messages sent: {}", sim.stats.messages_sent);
    println!("  Messages delivered: {}", sim.stats.messages_delivered);
    println!("  Direct deliveries: {}", sim.stats.direct_deliveries);
    println!("  Relayed deliveries: {}", sim.stats.relayed_deliveries);
    println!("  Back-props completed: {}", sim.stats.backprops_completed);
    println!("  Total hops: {}", sim.stats.total_hops);

    // Print event log
    println!("\n=== Event Log ===");
    for event in &sim.event_log {
        println!("  {:?}", event);
    }

    sim
}

/// Scenario: Multi-hop relay through a line topology
/// A - B - C - D - E
/// A sends to E when only A and B are online
pub fn run_line_relay_scenario() -> Simulation {
    info!("=== Running Line Relay Scenario ===");
    
    let mesh = from_edges(&[
        ('A', 'B'),
        ('B', 'C'),
        ('C', 'D'),
        ('D', 'E'),
    ]);

    println!("{}", mesh.visualize());

    let mut sim = Simulation::new(mesh, SimConfig {
        wake_probability: 0.0,
        sleep_probability: 0.0,
        trace_routing: true,
        ..Default::default()
    });

    // Start with A, B online
    sim.force_online(PeerId('A'));
    sim.force_online(PeerId('B'));

    println!("\n--- A sends to E (only A, B online) ---");
    sim.send_message(PeerId('A'), PeerId('E'), b"Multi-hop test".to_vec());
    sim.run_ticks(3);
    println!("  {}", sim.state_summary());

    // Wake peers one by one
    for peer in ['C', 'D', 'E'] {
        println!("\n--- {} comes online ---", peer);
        sim.force_online(PeerId(peer));
        sim.run_ticks(3);
        println!("  {}", sim.state_summary());
    }

    println!("\n=== Final Statistics ===");
    println!("  Messages delivered: {}", sim.stats.messages_delivered);
    println!("  Relayed deliveries: {}", sim.stats.relayed_deliveries);
    println!("  Total hops: {}", sim.stats.total_hops);

    sim
}

/// Scenario: Gossip-style broadcast to multiple destinations
/// Hub-and-spoke: A in center sends to all others
pub fn run_broadcast_scenario() -> Simulation {
    info!("=== Running Broadcast Scenario ===");

    use crate::topology::MeshBuilder;
    let mesh = MeshBuilder::new(5).star();

    println!("{}", mesh.visualize());

    let mut sim = Simulation::new(mesh, SimConfig {
        wake_probability: 0.0,
        sleep_probability: 0.0,
        trace_routing: true,
        ..Default::default()
    });

    // All peers online
    for c in 'A'..='E' {
        sim.force_online(PeerId(c));
    }

    // A broadcasts to all
    println!("\n--- A broadcasts to B, C, D, E ---");
    for dest in ['B', 'C', 'D', 'E'] {
        sim.send_message(PeerId('A'), PeerId(dest), format!("Broadcast to {}", dest).into_bytes());
    }

    sim.run_ticks(5);

    println!("\n=== Final Statistics ===");
    println!("  Messages sent: {}", sim.stats.messages_sent);
    println!("  Messages delivered: {}", sim.stats.messages_delivered);
    println!("  Direct deliveries: {}", sim.stats.direct_deliveries);

    sim
}

/// Scenario: Random network with probabilistic online/offline transitions
pub fn run_random_chaos_scenario(ticks: u64) -> Simulation {
    info!("=== Running Random Chaos Scenario ({} ticks) ===", ticks);

    use crate::topology::MeshBuilder;
    let mesh = MeshBuilder::new(8).random(0.4);

    println!("{}", mesh.visualize());

    let mut sim = Simulation::new(mesh, SimConfig {
        wake_probability: 0.3,
        sleep_probability: 0.2,
        initial_online_probability: 0.5,
        max_ticks: ticks,
        trace_routing: false, // Less verbose for chaos
        ..Default::default()
    });

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
                sim.send_message(from, to, format!("Message at tick {}", sim.tick).into_bytes());
            }
        }

        if sim.tick.is_multiple_of(20) {
            println!("Tick {}: {}", sim.tick, sim.state_summary());
        }
    }

    println!("\n=== Final Statistics ===");
    println!("  Messages sent: {}", sim.stats.messages_sent);
    println!("  Messages delivered: {}", sim.stats.messages_delivered);
    println!("  Messages dropped: {}", sim.stats.messages_dropped);
    println!("  Direct deliveries: {}", sim.stats.direct_deliveries);
    println!("  Relayed deliveries: {}", sim.stats.relayed_deliveries);
    println!("  Back-props completed: {}", sim.stats.backprops_completed);
    println!("  Wake events: {}", sim.stats.wake_events);
    println!("  Sleep events: {}", sim.stats.sleep_events);

    let delivery_rate = if sim.stats.messages_sent > 0 {
        sim.stats.messages_delivered as f64 / sim.stats.messages_sent as f64 * 100.0
    } else {
        0.0
    };
    println!("  Delivery rate: {:.1}%", delivery_rate);

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
        ('A', 'B'), ('A', 'C'), ('B', 'C'),
        // Cluster 2
        ('D', 'E'), ('D', 'F'), ('E', 'F'),
        // Bridge
        ('C', 'D'),
    ]);

    println!("{}", mesh.visualize());

    let mut sim = Simulation::new(mesh, SimConfig {
        wake_probability: 0.0,
        sleep_probability: 0.0,
        trace_routing: true,
        ..Default::default()
    });

    // All online initially
    for c in 'A'..='F' {
        sim.force_online(PeerId(c));
    }

    println!("\n--- A sends to F (full connectivity) ---");
    sim.send_message(PeerId('A'), PeerId('F'), b"Cross-cluster message".to_vec());
    sim.run_ticks(5);
    println!("  Delivered: {}", sim.stats.messages_delivered);

    println!("\n--- Bridge nodes C and D go offline (partition) ---");
    sim.force_offline(PeerId('C'));
    sim.force_offline(PeerId('D'));

    println!("\n--- A tries to send to F (partitioned) ---");
    sim.send_message(PeerId('A'), PeerId('F'), b"Message during partition".to_vec());
    sim.run_ticks(5);
    println!("  Delivered: {}", sim.stats.messages_delivered);
    println!("  (Should be held, not delivered)");

    println!("\n--- Bridge reconnects (C, D online) ---");
    sim.force_online(PeerId('C'));
    sim.force_online(PeerId('D'));
    sim.run_ticks(10);

    println!("\n=== Final Statistics ===");
    println!("  Messages sent: {}", sim.stats.messages_sent);
    println!("  Messages delivered: {}", sim.stats.messages_delivered);
    println!("  Messages dropped: {}", sim.stats.messages_dropped);

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
