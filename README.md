# Indra's Network

A mesh network simulation with store-and-forward routing for offline peers, inspired by [iroh-examples](https://github.com/n0-computer/iroh-examples).

## Overview

Indra's Network models peer-to-peer signal propagation where peers may go online and offline at any time. When a message cannot be delivered directly (destination offline), intermediate peers hold and forward the packet.

### Key Features

- **Named Peers (A-Z)**: Each node has a unique character identifier
- **Bidirectional Connections**: All peer pairings maintain synced interfaces
- **Event-Driven Architecture**: Append-only event logs (CRDT-style, like Automerge)
- **Store-and-Forward Routing**: Messages to offline peers are sealed and relayed
- **Back-Propagation**: Delivery confirmations travel back to the source

## Architecture

The design follows patterns from iroh-examples:

| Module | Inspired By | Purpose |
|--------|-------------|---------|
| `types.rs` | `browser-chat/shared` | Core data structures (PeerId, SealedPacket, Events) |
| `topology.rs` | - | Mesh network construction (ring, full, random) |
| `simulation.rs` | `iroh-automerge` sync protocol | Discrete-time simulation engine |
| `scenarios.rs` | - | Pre-built test scenarios |

### Data Flow

```
┌─────────────────────────────────────────────────────────────┐
│                    Indra's Network                          │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   Peer A ──────────── Interface(A,B) ──────────── Peer B   │
│     │                  ├─ Event Log                 │      │
│     │                  └─ Pending Packets           │      │
│     │                                               │      │
│     └──────────────── Interface(A,C) ──────────────┘      │
│                        ├─ Event Log                        │
│                        └─ Pending Packets                  │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

## The A-B-C Scenario

The canonical example from the spec:

```
1. A wakes up
2. B wakes up, messages A awake
3. A needs to send to C but C is asleep
   → A passes sealed message to B (mutual peer)
4. A goes to sleep
5. C wakes, messages B awake
6. B sees packet addressed to C, delivers it
7. B back-propagates "delivered" to A (when A next connects)
8. B deletes the relayed packet
```

## Installation

```bash
# Clone the repository
git clone <your-repo>
cd indras-network

# Build
cargo build --release

# Run the A-B-C scenario
cargo run -- abc

# Run interactive mode
cargo run -- interactive
```

## Usage

### CLI Commands

```bash
# Run pre-defined scenarios
cargo run -- abc        # The canonical A-B-C scenario
cargo run -- line       # Multi-hop line relay
cargo run -- broadcast  # Hub-and-spoke broadcast
cargo run -- chaos -t 200  # Random chaos for 200 ticks
cargo run -- partition  # Network partition and reconnect

# Visualize topologies
cargo run -- topology -t ring -p 8
cargo run -- topology -t full -p 5
cargo run -- topology -t random -p 10 -c 0.3

# Interactive mode
cargo run -- interactive -p 6
```

### Interactive Commands

```
online <peer>   - Bring peer online (e.g., 'online A')
offline <peer>  - Take peer offline
send <from> <to> <msg> - Send message
step [n]        - Advance n ticks (default 1)
status          - Show current state
stats           - Show statistics
events          - Show event log
quit            - Exit
```

### Library Usage

```rust
use indras_network::*;

// Create a mesh topology
let mesh = MeshBuilder::new(6).ring();

// Or from explicit edges
let mesh = from_edges(&[
    ('A', 'B'), ('B', 'C'), ('C', 'D'),
    ('A', 'D'), // shortcut
]);

// Create simulation
let mut sim = Simulation::new(mesh, SimConfig {
    wake_probability: 0.3,
    sleep_probability: 0.2,
    max_ticks: 100,
    ..Default::default()
});

// Initialize and run
sim.initialize();
sim.send_message(PeerId('A'), PeerId('F'), b"Hello!".to_vec());
sim.run();

// Check results
println!("Delivered: {}", sim.stats.messages_delivered);
```

## Design Decisions

### Why Sealed Packets?

In a real P2P network with iroh, packets would be encrypted for the destination. The "sealed" concept represents this - intermediate relays cannot read the content, only forward it.

### Why Back-Propagation?

After delivery, confirmation needs to flow back so:
1. The sender knows the message was delivered
2. Intermediate relays can delete their copies
3. The event log can be updated with delivery status

### Relation to Iroh Examples

| Iroh Example | Indra's Network Equivalent |
|--------------|---------------------------|
| `iroh-automerge-repo` | `PeerInterface.event_log` (CRDT sync) |
| `browser-chat/gossip` | Awake signals, presence detection |
| `framed-messages` | `SealedPacket` structure |

## Next Steps: Real P2P Implementation

To extend this to real iroh P2P:

1. Replace `PeerId(char)` with `iroh::EndpointId`
2. Use `iroh-gossip` for awake/presence signals
3. Use `automerge` + `samod` for synced event logs
4. Add encryption (box sealing) for sealed packets
5. Persist state with `TokioFilesystemStorage`

See the `iroh-automerge-repo` example for the foundation.

## License

MIT
