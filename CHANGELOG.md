# Changelog

All notable changes to Indra's Network are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.0.0] - 2026-01-23

### Initial Public Release

Indra's Network v1.0.0 is the first public release of a complete peer-to-peer mesh networking library built on the Iroh framework. The codebase represents a sophisticated implementation of delay-tolerant networking (DTN) with store-and-forward routing, end-to-end encryption, and CRDT-based document synchronization.

---

## Added

### Core Networking
- **Store-and-Forward Routing**: Automatic message persistence for offline peers with intelligent routing decisions (DIRECT, HOLD, RELAY, DROP)
- **Back-Propagation**: Delivery confirmations flow back through relay paths to confirm message success and enable garbage collection
- **Mutual Peer Tracking**: Maintains awareness of shared connections to route through optimal relay candidates
- **Route Caching**: Optimizes repeated routing decisions based on historical topology
- **Bidirectional Interfaces**: All peer connections maintain synchronized event logs and pending packet queues

### Delay-Tolerant Networking (DTN)
- **Multiple Routing Strategies**:
  - Store-and-Forward: Default strategy for connected networks
  - Epidemic: Flood-based routing for maximizing delivery probability in challenged networks
  - Spray-and-Wait: Limited-copy strategy with configurable spray count
- **Custody Transfer**: Nodes accept responsibility for bundle delivery with acknowledgments
- **Age-Based Priority Demotion**: Automatic priority reduction for aged bundles (configurable threshold)
- **Bundle Lifetime Management**: Configurable lifetimes from 5 minutes to 7 days
- **DTN Configuration Presets**: Pre-configured settings for low-latency networks, challenged networks, and resource-constrained devices

### Cryptography & Security
- **X25519 ECDH Key Exchange**: Secure establishment of shared secrets between peers
- **ChaCha20-Poly1305 AEAD Encryption**: Authenticated encryption for all inter-peer communication
- **Interface Key Management**: Group encryption keys for N-peer interfaces
- **Key Invites**: Secure onboarding mechanism for new interface members
- **Post-Quantum Cryptography Support**: ML-KEM-768 (key encapsulation) and ML-DSA-65 (digital signatures) for future-proofing
- **Sealed Packets**: Opaque packet structure preventing intermediate relays from reading encrypted content

### N-Peer Synchronization
- **Dual Synchronization Strategy**:
  - Event Broadcasting: Lightweight gossip-based publication for real-time events
  - Document Sync: Full state synchronization using Automerge CRDT
- **Automerge Document Backing**: Conflict-free document merging with automatic conflict resolution
- **Event Logs**: CRDT-style append-only logs with monotonic sequence numbers
- **Gossip Topic Subscription**: Topic-based pub/sub for selective message delivery
- **Offline Buffering**: Store-and-forward backup ensures offline peers receive updates on reconnect

### Transport & Peer Discovery
- **Iroh Integration**: Built on Iroh 0.95 for QUIC transport and NAT traversal
- **Endpoint Management**: Automatic Iroh endpoint lifecycle management
- **Peer Discovery**: Integration with Iroh's discovery mechanisms
- **QUIC Connections**: Multiplexed, encrypted connections between peers
- **Hole Punching Support**: Automatic NAT traversal for peer connectivity

### Storage & Quotas
- **Pending Event Storage**: Reliable storage for messages awaiting delivery
- **Quota Management**: Per-peer and global packet storage limits
- **Eviction Policies**: LRU (Least Recently Used) eviction when quotas are exceeded
- **Configurable Thresholds**: Flexible per-device and per-interface quotas

### IoT & Resource-Constrained Device Support
- **Low-Memory Optimizations**: Compact binary serialization with postcard
- **Duty Cycling Support**: Battery-efficient peer management
- **Lightweight Formats**: Minimal overhead for constrained networks
- **Note Broadcast Integration**: IoT module integration with example applications

### Simulation & Testing
- **Discrete-Event Simulation Engine**: Full network simulation with configurable time steps
- **Topology Definitions**: Ring, full mesh, and random graph topologies
- **Pre-Built Scenarios**:
  - ABC: Canonical 3-peer relay test (A→C through B)
  - Line: Multi-hop relay across linear topology
  - Broadcast: Hub-and-spoke message distribution
  - Chaos: Random network state changes
  - Partition: Network split and reconnect scenarios
  - Offline Relay: Mutual peer fallback when direct connection unavailable
- **Comprehensive Logging**: Structured event tracking throughout simulation
- **Statistics Collection**: Message delivery rates, latency, relay counts, and more
- **Interactive Mode**: Real-time peer control and state inspection
- **Unit Tests**: Full coverage across all modules with mock implementations

### Examples & Applications
- **Chat Application**: Multi-room chat with peer presence detection
- **Sync Demo**: Standalone example of Automerge document synchronization
- **Indra's Notes**: Feature-rich note-taking app with:
  - Collaborative note editing with real-time sync
  - Lua scripting support for custom behaviors
  - Note broadcast across peers
  - Structured logging with log analysis tools
  - Stress testing and performance analysis

### Monitoring & Observability
- **Structured Logging**: Comprehensive logging infrastructure with tracing
- **Log Analysis Tools**: Python-based log analysis and visualization
- **Stress Testing Framework**: Lua-scriptable test scenarios for:
  - Engine performance analysis
  - Gossip propagation testing
  - Logging throughput verification
  - Routing behavior validation
  - Storage capacity limits
  - Sync protocol performance
  - Transport connection handling
  - Scalability limits

### Dashboard & Visualization
- **Web Dashboard**: Indra's Dashboard web interface (Leptos-based)
- **Documents Tab**: CRDT document visualization
- **Theme System**: Light/dark theme support with CSS theming
- **Real-time State Display**: Live peer status and interface monitoring

### Documentation
- **README**: Comprehensive project overview and usage guide
- **ARCHITECTURE.md**: Detailed system architecture and design patterns
- **API Documentation**: Inline documentation for all public APIs
- **Code Examples**: Runnable examples for common use cases

---

## Architecture Overview

The v1.0.0 release includes a complete layered architecture:

```
┌─────────────────────────────────────────────────────────┐
│              Indra's Network Stack (v1.0.0)            │
├─────────────────────────────────────────────────────────┤
│  Application Layer                                      │
│  └─ Chat App, Sync Demo, Indra's Notes, Dashboard     │
│                                                        │
│  Messaging & Sync Layer                                │
│  ├─ indras-messaging   (High-level client API)         │
│  ├─ indras-sync        (Automerge CRDT sync)           │
│  └─ indras-gossip      (Topic-based pub/sub)           │
│                                                        │
│  Routing & Transport Layer                             │
│  ├─ indras-routing     (Store-forward, back-prop)      │
│  ├─ indras-dtn         (DTN routing strategies)        │
│  └─ indras-transport   (Iroh QUIC connections)         │
│                                                        │
│  Core & Utilities Layer                                │
│  ├─ indras-core        (Traits, types, abstractions)   │
│  ├─ indras-crypto      (X25519, ChaCha20-Poly1305)    │
│  └─ indras-storage     (Packet/event storage)          │
│                                                        │
│  Optimization Layer                                    │
│  └─ indras-iot         (Resource-constrained support)  │
│                                                        │
│  Simulation Layer                                      │
│  └─ simulation         (Discrete-event testing)        │
└─────────────────────────────────────────────────────────┘
```

---

## Key Implementation Details

### Module Responsibilities

| Crate | Purpose | Status |
|-------|---------|--------|
| **indras-core** | Foundation traits, types, errors | Complete |
| **indras-crypto** | Key management, AEAD encryption | Complete |
| **indras-transport** | Iroh endpoint, QUIC, peer discovery | Complete |
| **indras-routing** | Store-forward, mutual peers, back-prop | Complete |
| **indras-storage** | Packet storage, quota management | Complete |
| **indras-gossip** | Topic-based pub/sub, signed messages | Complete |
| **indras-sync** | N-peer interfaces, Automerge, events | Complete |
| **indras-messaging** | High-level send/receive API | Complete |
| **indras-dtn** | Bundle protocol, routing strategies | Complete |
| **indras-iot** | Low-memory, duty-cycling support | Complete |
| **indras-node** | Node coordinator, transport wrapper | Complete |
| **indras-dashboard** | Web UI for monitoring and visualization | Complete |
| **simulation** | Discrete-event simulation engine | Complete |

### Design Patterns

- **Trait-Based Abstraction**: All components use traits for loose coupling and testability
- **Generic Identity Types**: Support both character-based (simulation) and cryptographic (production) identities
- **Strategy Pattern**: Runtime selection of DTN routing strategies based on network conditions
- **CRDT-Based Synchronization**: Conflict-free document merging using Automerge
- **Sealed Packets**: Opaque intermediate relay structure preventing message exposure

---

## Technology Stack

| Category | Technology | Version |
|----------|-----------|---------|
| **Language** | Rust | 2024 Edition |
| **Runtime** | Tokio | 1.47 |
| **P2P Framework** | Iroh | 0.95 |
| **CRDT** | Automerge | 0.7 |
| **Serialization** | Postcard | 1.0 |
| **Encryption** | x25519-dalek, chacha20poly1305 | Latest |
| **Web Framework** | Leptos (Dashboard) | Latest |
| **Scripting** | Lua | 5.4 |
| **Concurrency** | DashMap | 6.0 |

---

## Testing Coverage

### Unit Tests
Complete unit test coverage across all modules:
- Error handling and edge cases
- Event routing and synchronization
- Packet storage and quota management
- Encryption and key exchange
- Type serialization and deserialization

### Integration Tests
- Multi-peer scenarios (A-B-C relay, line topologies)
- Chaos and partition scenarios
- Offline relay with mutual peer fallback
- Network partition and reconnection

### Simulation Tests
- Discrete-event simulation with configurable topologies
- Pre-built scenarios (ABC, line, broadcast, chaos, partition)
- Statistics collection and analysis
- Stress testing with Lua scripting

---

## Known Limitations (v1.0.0)

- **PRoPHET Routing**: Probabilistic routing based on encounter history not yet implemented (planned for v0.2.0)
- **Advanced IoT Features**: Full duty-cycling and extreme resource constraints deferred to Phase 5
- **Production Deployment**: While feature-complete, v1.0.0 is optimized for testing and simulation. Production deployments should conduct security audits
- **Platform Support**: Tested on Linux and macOS; Windows support pending verification

---

## Breaking Changes

None. This is the initial public release.

---

## Migration Guide

N/A - Initial release.

---

## Credits

Indra's Network is inspired by:
- **Iroh Examples** (https://github.com/n0-computer/iroh-examples): Transport and P2P patterns
- **Automerge** (https://automerge.org/): CRDT document synchronization
- **Indra's Net** Philosophy: Buddhist metaphor for interconnected systems

The project name references Indra's Net from Buddhist and Hindu philosophy, where each jewel reflects all others—directly informing the mesh networking architecture.

---

## License

Licensed under either of:
- MIT License (LICENSE-MIT or http://opensource.org/licenses/MIT)
- Apache License, Version 2.0 (LICENSE-APACHE or http://www.apache.org/licenses/LICENSE-2.0)

at your option.

---

## Future Roadmap

### v0.2.0 (Planned)
- PRoPHET routing algorithm implementation
- Enhanced statistics and analytics
- Performance optimizations

### v0.3.0 (Planned)
- Advanced reliability protocols
- Multi-interface orchestration
- Extended IoT support

### v1.0.0 (Long-term)
- Production-grade security audits
- Performance benchmarks and optimization
- Comprehensive deployment guide
