# Indra's Network

## Overview

Indra's Network is a sophisticated peer-to-peer mesh networking library with store-and-forward routing for offline peers. Built for **delay-tolerant networks (DTN)** and intermittently-connected systems, it provides a complete stack for peer discovery, routing, messaging, and synchronization on top of the [Iroh](https://iroh.computer/) framework.

### The Philosophy: Indra's Net

The name is an intentional allusion to **Indra's Net**—a metaphor from Buddhist and Hindu philosophy describing an infinite net of jewels, where each jewel at every intersection reflects all other jewels in the net. This concept directly informs the architecture:

| Philosophical Concept | Architectural Manifestation |
|-----------------------|----------------------------|
| **Each jewel reflects all others** | Every peer maintains awareness of its neighbors and propagates network state through gossip |
| **Infinite interconnection** | N-peer interfaces allow arbitrary group sizes with full mesh communication |
| **No center, no edge** | Pure P2P design with no central server or coordinator |
| **Light bouncing between jewels** | Back-propagation flows delivery confirmations through relay paths |
| **Multiple paths of reflection** | Epidemic routing floods multiple paths; mutual peer tracking enables diverse relay candidates |

---

## Core Capabilities

### Store-and-Forward Routing
Messages destined for offline peers are stored by intermediate nodes and delivered when the destination comes online. The routing algorithm follows a 4-step decision tree:

1. **DIRECT**: Destination is online and directly connected → deliver immediately
2. **HOLD**: Destination is offline but we're directly connected → store for later
3. **RELAY**: Destination is unreachable → find mutual peers to relay through
4. **DROP**: No route available → discard with reason (TTL expired, no route, no relay)

### Delay-Tolerant Networking (DTN)
Full DTN support with multiple routing strategies:

- **StoreAndForward** (default): Single-copy routing for connected networks
- **Epidemic**: Flood-based routing maximizing delivery probability
- **SprayAndWait**: Limited copy count (configurable), then wait for confirmation
- **PRoPHET**: Probabilistic routing based on encounter history (planned)

Additional DTN features include custody transfer (nodes accept delivery responsibility), age-based priority demotion, and configurable bundle lifetimes up to 7 days.

### N-Peer Synchronization (Dual Strategy)

**Real-time Events**: Lightweight event broadcasting via gossip, with store-and-forward backup for offline peers. Events are appended to a CRDT-style log with monotonic sequence numbers.

**Document Sync**: Full state synchronization using [Automerge](https://automerge.org/) for shared data, membership, and settings. Automatic conflict resolution when peers reconnect.

### End-to-End Encryption
- **X25519 ECDH** key exchange for establishing shared secrets
- **ChaCha20-Poly1305** authenticated encryption (AEAD)
- **Interface keys** for group encryption
- **Key invites** for secure onboarding of new members

---

## Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                        Indra's Network Stack                        │
├─────────────────────────────────────────────────────────────────────┤
│  Application Layer                                                  │
│  └─ Chat App, Sync Demo, Examples                                   │
│                                                                     │
│  Messaging & Sync Layer                                             │
│  ├─ indras-messaging   (High-level API)                             │
│  ├─ indras-sync        (Automerge CRDT sync)                        │
│  └─ indras-gossip      (Topic-based pub/sub)                        │
│                                                                     │
│  Routing & Transport Layer                                          │
│  ├─ indras-routing     (Store-forward, back-propagation)            │
│  ├─ indras-dtn         (DTN: epidemic, custody, aging)              │
│  └─ indras-transport   (Iroh QUIC connections)                      │
│                                                                     │
│  Core & Utilities Layer                                             │
│  ├─ indras-core        (Traits, types, abstractions)                │
│  ├─ indras-crypto      (X25519, ChaCha20-Poly1305)                  │
│  └─ indras-storage     (Packet/pending event storage)               │
│                                                                     │
│  Optimization Layer                                                 │
│  └─ indras-iot         (Resource-constrained support - Phase 5)     │
│                                                                     │
│  Simulation Layer                                                   │
│  └─ simulation         (Discrete-event testing)                     │
└─────────────────────────────────────────────────────────────────────┘
```

### Module Responsibilities

| Crate | Purpose |
|-------|---------|
| **indras-core** | Foundation traits (`PeerIdentity`, `NetworkTopology`, `Router`, `PacketStore`), packet types, routing decisions |
| **indras-crypto** | Interface key management, X25519 key exchange, ChaCha20-Poly1305 encryption, key invites |
| **indras-transport** | Iroh endpoint management, QUIC connections, peer discovery, wire protocol |
| **indras-routing** | Store-forward router, mutual peer tracking, back-propagation, route caching |
| **indras-storage** | Pending event storage, quota management, eviction policies |
| **indras-gossip** | Topic-based pub/sub, signed messages, split sender/receiver handles |
| **indras-sync** | N-peer interfaces, Automerge document backing, event logs, sync protocol |
| **indras-dtn** | Bundle protocol, epidemic/spray-and-wait routing, custody transfer, age management |
| **indras-messaging** | High-level client API for sending/receiving messages |
| **indras-iot** | (Planned) Low-memory, duty-cycling, compact formats for IoT devices |
| **simulation** | Discrete-event simulation engine, test scenarios, topology definitions |

---

## Data Flow

### Message Delivery (A→C when C is offline, B is mutual peer)

```
A sends to C:
  │
  ├─ 1. Check topology: C is offline, not directly connected
  │
  ├─ 2. Query mutual peers: B connects to both A and C
  │
  ├─ 3. Create Packet with routing hints [B]
  │
  ├─ 4. Route decision: RELAY through B
  │
  └─ 5. Send to B via QUIC
           │
           └─ B stores packet for C
                    │
                    └─ C comes online, signals B
                             │
                             └─ B delivers packet to C
                                      │
                                      └─ C sends back-prop to B
                                               │
                                               └─ B back-props to A
                                                        │
                                                        └─ A confirms delivery
```

### N-Peer Interface Sync

```
Interface Members: A, B, C

A appends message:
  │
  ├─ 1. Create InterfaceEvent::Message
  │
  ├─ 2. Append to local Automerge document
  │
  ├─ 3. Broadcast via gossip topic (unreliable, fast)
  │
  └─ 4. Store-and-forward to B, C (reliable, for offline)
           │
           ├─ B receives (online) → confirms via back-prop
           │
           └─ C receives (was offline) → syncs on reconnect
                    │
                    └─ Automerge sync resolves any conflicts
```

---

## Key Design Patterns

### Trait-Based Abstraction
Everything is built on traits for loose coupling and testability:

```rust
// Same router logic works with test topologies AND real iroh endpoints
pub trait NetworkTopology<I: PeerIdentity>: Send + Sync {
    fn peers(&self) -> Vec<I>;
    fn neighbors(&self, peer: &I) -> Vec<I>;
    fn are_connected(&self, a: &I, b: &I) -> bool;
    fn is_online(&self, peer: &I) -> bool;
}
```

### Generic Identity Type
All types are generic over `I: PeerIdentity`:

- **Simulation**: Uses `char`-based identities ('A', 'B', 'C') for fast, readable testing
- **Production**: Uses cryptographic `PublicKey` from Iroh

```rust
// Same business logic works with both identity types
Packet<I>, RoutingDecision<I>, InterfaceEvent<I>, Message<I>
```

### Sealed Packets
Intermediate relays cannot read message contents:

```rust
pub struct EncryptedPayload {
    pub data: Vec<u8>,
    pub encrypted: bool,  // true in production, false in simulation
}
```

### Strategy Pattern for DTN
Runtime selection of routing strategy based on network conditions:

```rust
if network_connectivity < 30% {
    use Epidemic        // Maximize delivery probability
} else if bundle.is_critical() {
    use Epidemic        // Critical messages always flood
} else if bundle.age > 10_minutes {
    use SprayAndWait    // Limit copies for old bundles
} else {
    use StoreAndForward // Default for connected networks
}
```

---

## Technology Stack

| Category | Technology | Purpose |
|----------|------------|---------|
| **Language** | Rust 2024 Edition | Memory safety, async/await |
| **Runtime** | Tokio 1.47 | Multi-threaded async executor |
| **P2P** | Iroh 0.95 | QUIC transport, NAT traversal, hole punching |
| **CRDT** | Automerge 0.7 | Conflict-free document synchronization |
| **Crypto** | x25519-dalek + chacha20poly1305 | Key exchange + AEAD encryption |
| **Serialization** | Postcard 1.0 | Compact binary wire format |
| **Concurrency** | DashMap 6.0 | Lock-free concurrent hashmaps |

---

## Configuration

### DTN Presets

```rust
// Low-latency networks (good connectivity)
DtnConfig::low_latency()     // 600s lifetime, StoreAndForward

// Challenged networks (intermittent connectivity)
DtnConfig::challenged_network()  // 1 day lifetime, Epidemic, 16 max copies

// Resource-constrained devices
DtnConfig::resource_constrained()  // 300s lifetime, spray_count=2
```

### Quota Management

```rust
QuotaManagerBuilder::new()
    .max_total(1000)           // Max packets in storage
    .max_per_peer(100)         // Max per destination
    .eviction(EvictionPolicy::LRU)
    .build()
```

---

## Testing

The project uses a three-tier testing strategy:

1. **Unit Tests**: Every module has `#[cfg(test)]` with mock implementations
2. **Integration Tests**: Multi-peer scenarios (A-B-C relay, line topology, chaos)
3. **Discrete-Event Simulation**: Full network simulation with configurable topology

```rust
// Simulation example
let mut sim = Simulation::new(mesh, config);
sim.force_online(peer_a);
sim.send_message(peer_a, peer_c, b"Hello".to_vec());
sim.run_ticks(10);
assert_eq!(sim.stats.messages_delivered, 1);
```

---

## Project Status

| Component | Status |
|-----------|--------|
| Core routing | ✅ Complete |
| Store-and-forward | ✅ Complete |
| Back-propagation | ✅ Complete |
| DTN (epidemic, spray-and-wait) | ✅ Complete |
| Custody transfer | ✅ Complete |
| N-peer interfaces | ✅ Complete |
| Automerge sync | ✅ Complete |
| Iroh transport | ✅ Complete |
| Encryption | ✅ Complete |
| IoT optimizations | ⏳ Phase 5 |
| PRoPHET routing | ⏳ Planned |

---

## License

MIT OR Apache-2.0 (dual license)
