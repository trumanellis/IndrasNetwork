//! Fraud proof generation and verification.
//!
//! Detects equivocation: when an author produces two different events
//! with the same sequence number. This is the primary fraud vector
//! in the locally-conservative attention ledger.

use crate::artifact::PlayerId;
use crate::attention::AttentionSwitchEvent;
use serde::{Deserialize, Serialize};

/// Evidence of equivocation by an author.
///
/// Contains two events from the same author with the same sequence number
/// but different content (different event hashes).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EquivocationProof {
    /// The author who equivocated.
    pub author: PlayerId,
    /// The sequence number where equivocation occurred.
    pub seq: u64,
    /// First observed event at this (author, seq).
    pub event_a: AttentionSwitchEvent,
    /// Second (conflicting) event at this (author, seq).
    pub event_b: AttentionSwitchEvent,
}

impl EquivocationProof {
    /// Verify that this proof is structurally valid.
    ///
    /// Both events must share the same author and seq but have different hashes.
    pub fn is_valid(&self) -> bool {
        self.event_a.author == self.event_b.author
            && self.event_a.author == self.author
            && self.event_a.seq == self.event_b.seq
            && self.event_a.seq == self.seq
            && self.event_a.event_hash() != self.event_b.event_hash()
    }

    /// Verify signatures on both events (proves the author actually signed both).
    pub fn verify_signatures(&self, public_key: &indras_crypto::PQPublicIdentity) -> bool {
        self.event_a.verify_signature(public_key) && self.event_b.verify_signature(public_key)
    }
}

/// Check a new event against existing events for equivocation.
///
/// Returns an [`EquivocationProof`] if the new event conflicts with an
/// existing event at the same (author, seq).
pub fn check_equivocation(
    new_event: &AttentionSwitchEvent,
    existing_events: &[AttentionSwitchEvent],
) -> Option<EquivocationProof> {
    for existing in existing_events {
        if existing.author == new_event.author
            && existing.seq == new_event.seq
            && existing.event_hash() != new_event.event_hash()
        {
            return Some(EquivocationProof {
                author: new_event.author,
                seq: new_event.seq,
                event_a: existing.clone(),
                event_b: new_event.clone(),
            });
        }
    }
    None
}
