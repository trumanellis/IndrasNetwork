//! DTN Bundle - wraps a Packet with DTN-specific metadata
//!
//! A Bundle is the fundamental unit of data in DTN. It wraps an existing
//! Indras Packet with additional metadata for custody transfer, lifetime
//! management, and class of service.

use std::hash::{Hash, Hasher};

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use indras_core::{Packet, PeerIdentity, Priority};

/// Unique identifier for a DTN bundle
///
/// May differ from the inner packet ID to allow bundle-level tracking
/// independent of the payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BundleId {
    /// Hash of the source identity
    pub source_hash: u64,
    /// Creation timestamp in Unix milliseconds
    pub creation_timestamp: i64,
    /// Sequence number for bundles created at the same millisecond
    pub sequence: u32,
}

impl BundleId {
    /// Create a new bundle ID
    pub fn new(source_hash: u64, sequence: u32) -> Self {
        Self {
            source_hash,
            creation_timestamp: Utc::now().timestamp_millis(),
            sequence,
        }
    }

    /// Create from a packet (derives from packet ID)
    pub fn from_packet<I: PeerIdentity>(packet: &Packet<I>) -> Self {
        Self {
            source_hash: packet.id.source_hash,
            creation_timestamp: packet.created_at.timestamp_millis(),
            sequence: packet.id.sequence as u32,
        }
    }
}

impl std::fmt::Display for BundleId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:08x}@{}#{}",
            self.source_hash & 0xFFFF,
            self.creation_timestamp,
            self.sequence
        )
    }
}

/// Class of service for DTN bundles
///
/// Determines handling priority and resource allocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ClassOfService {
    /// Bulk transfer - lowest priority, high delay tolerance
    BulkTransfer,
    /// Normal delivery (default)
    #[default]
    Normal,
    /// Expedited - higher priority
    Expedited,
    /// Critical - highest priority, minimal delay acceptable
    Critical,
}

impl ClassOfService {
    /// Convert to Indras Priority
    pub fn to_priority(self) -> Priority {
        match self {
            ClassOfService::BulkTransfer => Priority::Low,
            ClassOfService::Normal => Priority::Normal,
            ClassOfService::Expedited => Priority::High,
            ClassOfService::Critical => Priority::Critical,
        }
    }

    /// Create from Indras Priority
    pub fn from_priority(priority: Priority) -> Self {
        match priority {
            Priority::Low => ClassOfService::BulkTransfer,
            Priority::Normal => ClassOfService::Normal,
            Priority::High => ClassOfService::Expedited,
            Priority::Critical => ClassOfService::Critical,
        }
    }
}

/// Record of a custody transfer
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(bound = "I: PeerIdentity")]
pub struct CustodyTransfer<I: PeerIdentity> {
    /// Node transferring custody
    pub from: I,
    /// Node receiving custody
    pub to: I,
    /// When the transfer occurred
    pub timestamp: DateTime<Utc>,
    /// Whether custody was accepted
    pub accepted: bool,
}

impl<I: PeerIdentity> CustodyTransfer<I> {
    /// Create a new custody transfer record
    pub fn new(from: I, to: I, accepted: bool) -> Self {
        Self {
            from,
            to,
            timestamp: Utc::now(),
            accepted,
        }
    }
}

/// Summary of a bundle for custody offers
///
/// Contains enough information to decide whether to accept custody
/// without transferring the full bundle.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(bound = "I: PeerIdentity")]
pub struct BundleSummary<I: PeerIdentity> {
    /// Bundle identifier
    pub bundle_id: BundleId,
    /// Final destination
    pub destination: I,
    /// Payload size in bytes
    pub payload_size: usize,
    /// Class of service
    pub class_of_service: ClassOfService,
    /// Time remaining until expiration
    pub time_remaining: Duration,
    /// Number of custody transfers so far
    pub custody_hop_count: usize,
}

/// DTN Bundle - wraps a Packet with DTN-specific metadata
///
/// The Bundle adds delay-tolerant networking capabilities to the basic
/// Packet type, including:
/// - Lifetime-based expiration (in addition to TTL hops)
/// - Custody transfer support
/// - Class of service designation
/// - Delivery and custody reporting
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(bound = "I: PeerIdentity")]
pub struct Bundle<I: PeerIdentity> {
    /// Inner packet containing payload and routing info
    pub packet: Packet<I>,
    /// Bundle-specific identifier
    pub bundle_id: BundleId,
    /// Maximum lifetime before expiration (age-based)
    pub lifetime: Duration,
    /// Whether custody transfer is requested
    pub custody_requested: bool,
    /// Current custodian (who's responsible for delivery)
    pub current_custodian: Option<I>,
    /// History of custody transfers
    pub custody_history: Vec<CustodyTransfer<I>>,
    /// Bundle class of service
    pub class_of_service: ClassOfService,
    /// Whether to report delivery to source
    pub report_delivery: bool,
    /// Whether to report custody acceptance to source
    pub report_custody: bool,
    /// Remaining copies for spray-and-wait routing
    pub copies_remaining: u8,
}

impl<I: PeerIdentity> Bundle<I> {
    /// Create a bundle from an existing packet
    pub fn from_packet(packet: Packet<I>, lifetime: Duration) -> Self {
        let bundle_id = BundleId::from_packet(&packet);
        let class_of_service = ClassOfService::from_priority(packet.priority);

        Self {
            packet,
            bundle_id,
            lifetime,
            custody_requested: false,
            current_custodian: None,
            custody_history: Vec::new(),
            class_of_service,
            report_delivery: false,
            report_custody: false,
            copies_remaining: 1,
        }
    }

    /// Create a bundle with custody transfer enabled
    pub fn with_custody(mut self, initial_custodian: I) -> Self {
        self.custody_requested = true;
        self.current_custodian = Some(initial_custodian);
        self
    }

    /// Set the class of service
    pub fn with_class_of_service(mut self, cos: ClassOfService) -> Self {
        self.class_of_service = cos;
        self.packet.priority = cos.to_priority();
        self
    }

    /// Enable delivery reporting
    pub fn with_delivery_report(mut self) -> Self {
        self.report_delivery = true;
        self
    }

    /// Enable custody reporting
    pub fn with_custody_report(mut self) -> Self {
        self.report_custody = true;
        self
    }

    /// Set copies for spray-and-wait routing
    pub fn with_copies(mut self, copies: u8) -> Self {
        self.copies_remaining = copies;
        self
    }

    /// Check if the bundle has expired (age-based)
    pub fn is_expired(&self) -> bool {
        self.age() >= self.lifetime
    }

    /// Get the age of this bundle
    pub fn age(&self) -> Duration {
        Utc::now() - self.packet.created_at
    }

    /// Get remaining time before expiration
    pub fn time_to_live(&self) -> Duration {
        let age = self.age();
        if age >= self.lifetime {
            Duration::zero()
        } else {
            self.lifetime - age
        }
    }

    /// Transfer custody to a new node
    ///
    /// Records the transfer in history and updates current custodian.
    /// Returns the transfer record.
    pub fn transfer_custody(&mut self, to: I) -> Option<CustodyTransfer<I>> {
        if !self.custody_requested {
            return None;
        }

        let from = self.current_custodian.take()?;
        let transfer = CustodyTransfer::new(from, to.clone(), true);
        self.custody_history.push(transfer.clone());
        self.current_custodian = Some(to);

        Some(transfer)
    }

    /// Accept custody as the initial custodian
    pub fn accept_initial_custody(&mut self, custodian: I) {
        self.custody_requested = true;
        self.current_custodian = Some(custodian);
    }

    /// Get the effective priority considering class of service
    pub fn effective_priority(&self) -> Priority {
        self.class_of_service.to_priority()
    }

    /// Get the destination of this bundle
    pub fn destination(&self) -> &I {
        &self.packet.destination
    }

    /// Get the source of this bundle
    pub fn source(&self) -> &I {
        &self.packet.source
    }

    /// Get a summary of this bundle (for custody offers)
    pub fn summary(&self) -> BundleSummary<I> {
        BundleSummary {
            bundle_id: self.bundle_id,
            destination: self.packet.destination.clone(),
            payload_size: self.packet.payload.len(),
            class_of_service: self.class_of_service,
            time_remaining: self.time_to_live(),
            custody_hop_count: self.custody_history.len(),
        }
    }

    /// Decrement copies for spray-and-wait routing
    ///
    /// Returns true if there are copies remaining after decrement.
    pub fn decrement_copies(&mut self) -> bool {
        if self.copies_remaining > 1 {
            self.copies_remaining -= 1;
            true
        } else {
            false
        }
    }
}

impl<I: PeerIdentity> PartialEq for Bundle<I> {
    fn eq(&self, other: &Self) -> bool {
        self.bundle_id == other.bundle_id
    }
}

impl<I: PeerIdentity> Eq for Bundle<I> {}

impl<I: PeerIdentity> Hash for Bundle<I> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.bundle_id.hash(state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use indras_core::{EncryptedPayload, PacketId, SimulationIdentity};

    fn make_test_packet() -> Packet<SimulationIdentity> {
        let source = SimulationIdentity::new('A').unwrap();
        let dest = SimulationIdentity::new('Z').unwrap();
        let id = PacketId::new(0x1234, 1);

        Packet::new(
            id,
            source,
            dest,
            EncryptedPayload::plaintext(b"test payload".to_vec()),
            vec![],
        )
    }

    #[test]
    fn test_bundle_creation() {
        let packet = make_test_packet();
        let bundle = Bundle::from_packet(packet, Duration::hours(1));

        assert!(!bundle.is_expired());
        assert!(!bundle.custody_requested);
        assert!(bundle.current_custodian.is_none());
        assert_eq!(bundle.class_of_service, ClassOfService::Normal);
    }

    #[test]
    fn test_bundle_with_custody() {
        let packet = make_test_packet();
        let custodian = SimulationIdentity::new('B').unwrap();
        let bundle = Bundle::from_packet(packet, Duration::hours(1)).with_custody(custodian);

        assert!(bundle.custody_requested);
        assert!(bundle.current_custodian.is_some());
    }

    #[test]
    fn test_custody_transfer() {
        let packet = make_test_packet();
        let custodian_a = SimulationIdentity::new('A').unwrap();
        let custodian_b = SimulationIdentity::new('B').unwrap();

        let mut bundle = Bundle::from_packet(packet, Duration::hours(1)).with_custody(custodian_a);

        let transfer = bundle.transfer_custody(custodian_b);
        assert!(transfer.is_some());
        assert_eq!(bundle.custody_history.len(), 1);
        assert_eq!(
            bundle.current_custodian,
            Some(SimulationIdentity::new('B').unwrap())
        );
    }

    #[test]
    fn test_class_of_service_mapping() {
        assert_eq!(ClassOfService::BulkTransfer.to_priority(), Priority::Low);
        assert_eq!(ClassOfService::Normal.to_priority(), Priority::Normal);
        assert_eq!(ClassOfService::Expedited.to_priority(), Priority::High);
        assert_eq!(ClassOfService::Critical.to_priority(), Priority::Critical);

        assert_eq!(
            ClassOfService::from_priority(Priority::Low),
            ClassOfService::BulkTransfer
        );
        assert_eq!(
            ClassOfService::from_priority(Priority::Critical),
            ClassOfService::Critical
        );
    }

    #[test]
    fn test_bundle_summary() {
        let packet = make_test_packet();
        let bundle = Bundle::from_packet(packet, Duration::hours(1))
            .with_class_of_service(ClassOfService::Expedited);

        let summary = bundle.summary();
        assert_eq!(summary.bundle_id, bundle.bundle_id);
        assert_eq!(summary.class_of_service, ClassOfService::Expedited);
        assert_eq!(summary.custody_hop_count, 0);
    }

    #[test]
    fn test_copies_decrement() {
        let packet = make_test_packet();
        let mut bundle = Bundle::from_packet(packet, Duration::hours(1)).with_copies(3);

        assert_eq!(bundle.copies_remaining, 3);
        assert!(bundle.decrement_copies());
        assert_eq!(bundle.copies_remaining, 2);
        assert!(bundle.decrement_copies());
        assert_eq!(bundle.copies_remaining, 1);
        assert!(!bundle.decrement_copies()); // Can't go below 1
        assert_eq!(bundle.copies_remaining, 1);
    }
}
