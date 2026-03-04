//! Fraud evidence tracking for equivocation detection.
//!
//! When a peer detects equivocation (same author, same seq, different event),
//! they publish a fraud proof. This document collects all known fraud proofs
//! for a realm so that all members can see which authors have equivocated.
//!
//! # CRDT Semantics
//!
//! - Merge strategy: union of fraud proofs (keyed by author + seq)
//! - Fraud proofs are self-verifying (contain both conflicting events)

use indras_network::member::MemberId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A fraud proof stored in the evidence document.
///
/// Contains the two conflicting events (as serialized bytes) so any
/// peer can independently verify the equivocation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FraudRecord {
    /// The author who equivocated.
    pub author: MemberId,
    /// The sequence number where equivocation occurred.
    pub seq: u64,
    /// First event (postcard-serialized AttentionSwitchEvent from indras-artifacts).
    pub event_a_bytes: Vec<u8>,
    /// Second conflicting event (postcard-serialized).
    pub event_b_bytes: Vec<u8>,
    /// Who reported this fraud.
    pub reporter: MemberId,
    /// When the fraud was detected (wall-clock ms).
    pub detected_at_ms: i64,
}

/// Key for deduplicating fraud records: (author, seq).
type FraudKey = (MemberId, u64);

/// CRDT document collecting fraud evidence for a realm.
///
/// All members converge on the same set of fraud proofs via union merge.
/// An author appearing in this document has been caught equivocating and
/// their attention data should be considered unreliable.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct FraudEvidenceDocument {
    /// Fraud records keyed by (author, seq) for deduplication.
    records: HashMap<MemberId, Vec<FraudRecord>>,
}

impl FraudEvidenceDocument {
    /// Create a new empty fraud evidence document.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a fraud record. Deduplicates by (author, seq).
    pub fn add_record(&mut self, record: FraudRecord) {
        let author_records = self.records.entry(record.author).or_default();
        let already_exists = author_records.iter().any(|r| r.seq == record.seq);
        if !already_exists {
            author_records.push(record);
        }
    }

    /// Check if an author has any fraud records.
    pub fn is_fraudulent(&self, author: &MemberId) -> bool {
        self.records
            .get(author)
            .map_or(false, |records| !records.is_empty())
    }

    /// Get all fraud records for an author.
    pub fn records_for(&self, author: &MemberId) -> &[FraudRecord] {
        self.records
            .get(author)
            .map_or(&[], |records| records.as_slice())
    }

    /// Get all known fraudulent authors.
    pub fn fraudulent_authors(&self) -> Vec<MemberId> {
        self.records
            .iter()
            .filter(|(_, records)| !records.is_empty())
            .map(|(author, _)| *author)
            .collect()
    }

    /// Get all fraud records across all authors.
    pub fn all_records(&self) -> Vec<&FraudRecord> {
        self.records.values().flat_map(|v| v.iter()).collect()
    }

    /// Merge another fraud evidence document (union of records).
    pub fn merge(&mut self, other: Self) {
        for (author, records) in other.records {
            for record in records {
                self.add_record(record);
            }
        }
    }
}
