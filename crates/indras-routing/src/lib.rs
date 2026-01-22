//! # Indras Routing
//!
//! Routing layer for Indras Network.
//!
//! This crate implements store-and-forward routing with back-propagation
//! for delivery confirmations. It is designed to work with both simulation
//! identities (for testing) and real cryptographic identities.
//!
//! ## Core Components
//!
//! - [`StoreForwardRouter`]: Main router implementation that makes routing decisions
//! - [`MutualPeerTracker`]: Tracks mutual peers between connected peers for relay selection
//! - [`BackPropManager`]: Manages back-propagation of delivery confirmations
//! - [`RoutingTable`]: Caches route information with staleness detection
//!
//! ## Routing Algorithm
//!
//! The router uses a four-step decision process:
//!
//! 1. **DIRECT**: If destination is online and directly connected, deliver directly
//! 2. **HOLD**: If destination is offline but directly connected, store for later delivery
//! 3. **RELAY**: If not directly connected, use mutual peers as relay candidates
//! 4. **DROP**: If no route is available, drop the packet
//!
//! ## Store-and-Forward
//!
//! When a destination peer is offline, packets are stored locally and delivered
//! when the peer comes back online. This enables asynchronous communication
//! even when peers have intermittent connectivity.
//!
//! ## Back-Propagation
//!
//! When a packet is delivered, a confirmation is sent back along the path it took.
//! This allows intermediate relays and the source to know the packet was delivered.
//!
//! ## Example
//!
//! ```rust,ignore
//! use std::sync::Arc;
//! use indras_routing::{StoreForwardRouter, MutualPeerTracker};
//! use indras_core::{Router, Packet};
//!
//! // Create router with topology and storage
//! let router = StoreForwardRouter::new(topology, storage);
//!
//! // Notify router of peer connections (for mutual peer tracking)
//! router.on_peer_connect(&peer_a, &peer_b);
//!
//! // Route a packet
//! let decision = router.route(&packet, &current_peer).await?;
//! match decision {
//!     RoutingDecision::DirectDelivery { destination } => { /* deliver */ }
//!     RoutingDecision::RelayThrough { next_hops } => { /* relay */ }
//!     RoutingDecision::HoldForLater => { /* stored */ }
//!     RoutingDecision::Drop { reason } => { /* dropped */ }
//! }
//! ```

pub mod backprop;
pub mod error;
pub mod mutual;
pub mod router;
pub mod table;

// Re-export main types
pub use backprop::{BackPropManager, BackPropState, BackPropStatus};
pub use error::{RoutingError, RoutingResult};
pub use mutual::MutualPeerTracker;
pub use router::StoreForwardRouter;
pub use table::RoutingTable;

// Re-export core routing types for convenience
pub use indras_core::{DropReason, RouteInfo, RoutingDecision};
