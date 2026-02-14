//! # Indras Sync
//!
//! CRDT-based document synchronization using Yrs for N-peer interfaces.
//!
//! This crate provides the synchronization layer for the Indras Network,
//! combining Yrs document sync with store-and-forward event delivery.
//!
//! ## Key Components
//!
//! - [`InterfaceDocument`]: Yrs document backing an N-peer interface
//! - [`EventStore`]: Store-and-forward event storage with delivery tracking
//! - [`SyncProtocol`]: Sync protocol handlers and state management
//!
//! ## Dual Sync Strategy
//!
//! The Indras Network uses a dual synchronization strategy:
//!
//! 1. **Events (Store-and-Forward)**: Lightweight, real-time delivery of events.
//!    Events are held for offline peers until they reconnect and confirm receipt.
//!
//! 2. **Documents (Yrs Sync)**: Full state synchronization using Yrs's
//!    sync protocol. Handles membership, settings, and shared data with automatic
//!    conflict resolution.
//!
//! ## Example
//!
//! ```rust,ignore
//! use indras_sync::{InterfaceDocument, EventStore, SyncProtocol, SyncState};
//! use indras_core::{InterfaceEvent, SimulationIdentity, InterfaceId};
//!
//! // Create a new interface document
//! let mut doc = InterfaceDocument::new();
//!
//! // Add members
//! let peer_a = SimulationIdentity::new('A').unwrap();
//! let peer_b = SimulationIdentity::new('B').unwrap();
//! doc.add_member(&peer_a);
//! doc.add_member(&peer_b);
//!
//! // Append an event
//! let event = InterfaceEvent::message(peer_a, 1, b"Hello".to_vec());
//! doc.append_event(&event).unwrap();
//!
//! // Generate sync message for peer B
//! let interface_id = InterfaceId::generate();
//! let mut sync_state = SyncState::new(interface_id);
//! let sync_msg = SyncProtocol::generate_sync_request(
//!     interface_id,
//!     &doc,
//!     &mut sync_state,
//!     &peer_b,
//! );
//! ```

pub mod document;
pub mod error;
pub mod event_store;
pub mod n_interface;
pub mod sync_protocol;

// Re-exports
pub use document::InterfaceDocument;
pub use error::{SyncError, SyncResult};
pub use event_store::EventStore;
pub use n_interface::NInterface;
pub use sync_protocol::{PeerSyncState, PendingDelivery, SyncProtocol, SyncState};
