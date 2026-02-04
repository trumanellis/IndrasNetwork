//! Humanness attestation and Proof of Life system.
//!
//! Humanness is not a one-time credential but a heartbeat. Members are attested
//! as human through shared experiences — saving a Memory to a shared realm,
//! gathering for a meal, collaborating on a document. Each attestation refreshes
//! the member's humanness, which decays exponentially after a grace period.
//!
//! # Freshness Model
//!
//! - Full freshness (1.0) for the first 7 days after attestation
//! - Exponential decay after: `e^(-0.1 × (days - 7))`
//! - At 14 days: ~0.497, at 21 days: ~0.247, at 30 days: ~0.100
//!
//! # Bioregional Delegation Tree
//!
//! Humanness attestation authority flows through a fractal hierarchy based on
//! OneEarth's bioregional model:
//!
//! - Temples of Refuge (root — 1)
//! - Realm Temples (continental — 14)
//! - Subrealm Temples (subcontinental — 52)
//! - Bioregion Temples (regional — 185)
//! - Ecoregion Temples (local — 844)
//! - Individual attesters (people on the land)
//!
//! Each level delegates to the next via signed delegations. Trust in the chain
//! is subjective — each observer evaluates each link through their own sentiment.

use indras_network::member::MemberId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Grace period before freshness starts to decay (in days).
pub const FRESHNESS_GRACE_DAYS: f64 = 7.0;

/// Decay rate for humanness freshness after the grace period.
/// Higher values = faster decay.
pub const FRESHNESS_DECAY_RATE: f64 = 0.1;

/// Bioregional delegation hierarchy level.
///
/// Variants are ordered by hierarchy depth (Root = 0, Individual = 5).
/// The derived `PartialOrd`/`Ord` follow variant declaration order,
/// so `Root < Realm < Subrealm < Bioregion < Ecoregion < Individual`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum BioregionalLevel {
    /// Temples of Refuge — the root (1 worldwide).
    Root,
    /// Continental temples (14 worldwide).
    Realm,
    /// Subcontinental temples (52 worldwide).
    Subrealm,
    /// Regional temples (185 worldwide).
    Bioregion,
    /// Local temples (844 worldwide).
    Ecoregion,
    /// An individual attester on the land.
    Individual,
}

impl BioregionalLevel {
    /// Returns the depth of this level in the hierarchy (Root=0, Individual=5).
    pub fn depth(&self) -> u8 {
        match self {
            BioregionalLevel::Root => 0,
            BioregionalLevel::Realm => 1,
            BioregionalLevel::Subrealm => 2,
            BioregionalLevel::Bioregion => 3,
            BioregionalLevel::Ecoregion => 4,
            BioregionalLevel::Individual => 5,
        }
    }

    /// Returns the level one step above this one, or `None` if this is Root.
    pub fn parent_level(&self) -> Option<BioregionalLevel> {
        match self {
            BioregionalLevel::Root => None,
            BioregionalLevel::Realm => Some(BioregionalLevel::Root),
            BioregionalLevel::Subrealm => Some(BioregionalLevel::Realm),
            BioregionalLevel::Bioregion => Some(BioregionalLevel::Subrealm),
            BioregionalLevel::Ecoregion => Some(BioregionalLevel::Bioregion),
            BioregionalLevel::Individual => Some(BioregionalLevel::Ecoregion),
        }
    }
}

/// A delegation of humanness attestation authority.
///
/// Each delegation says: "I (delegator) trust this entity (delegate) to
/// attest humanness at this level of the bioregional hierarchy."
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Delegation {
    /// The entity granting the delegation.
    pub delegator: MemberId,
    /// The entity receiving the delegation.
    pub delegate: MemberId,
    /// The level in the bioregional hierarchy.
    pub level: BioregionalLevel,
    /// When this delegation was issued (Unix millis).
    pub timestamp_millis: i64,
}

/// A humanness attestation — proof that someone was recently present
/// in a shared human experience.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HumannessAttestation {
    /// The member being attested as human.
    pub subject: MemberId,
    /// The member (or institution) making the attestation.
    pub attester: MemberId,
    /// Chain of delegations from root to attester (may be empty for peer attestation).
    pub delegation_chain: Vec<Delegation>,
    /// When the attestation was recorded (Unix millis).
    pub timestamp_millis: i64,
}

/// Events in the humanness document lifecycle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HumannessEvent {
    /// A new attestation was recorded.
    Attested(HumannessAttestation),
    /// A proof of life celebration — all participants attested simultaneously.
    ProofOfLife {
        participants: Vec<MemberId>,
        attester: MemberId,
        timestamp_millis: i64,
    },
}

/// CRDT document for humanness tracking (realm-scoped).
///
/// Append-only event log with derived state: latest attestation timestamp
/// per member, rebuilt from events on merge.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HumannessDocument {
    /// Append-only event log.
    events: Vec<HumannessEvent>,
    /// Derived state: member -> latest attestation timestamp (millis).
    #[serde(default)]
    latest: HashMap<MemberId, i64>,
}

impl HumannessDocument {
    /// Create a new empty document.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a humanness attestation.
    pub fn attest(&mut self, attestation: HumannessAttestation) {
        let ts = attestation.timestamp_millis;
        let subject = attestation.subject;

        self.events.push(HumannessEvent::Attested(attestation));

        // Update derived state
        let entry = self.latest.entry(subject).or_insert(0);
        if ts > *entry {
            *entry = ts;
        }
    }

    /// Record a proof of life celebration — attests all participants at once.
    pub fn record_proof_of_life(
        &mut self,
        participants: Vec<MemberId>,
        attester: MemberId,
        timestamp_millis: i64,
    ) {
        self.events.push(HumannessEvent::ProofOfLife {
            participants: participants.clone(),
            attester,
            timestamp_millis,
        });

        // Update derived state for all participants
        for member in &participants {
            let entry = self.latest.entry(*member).or_insert(0);
            if timestamp_millis > *entry {
                *entry = timestamp_millis;
            }
        }
    }

    /// Get the latest attestation timestamp for a member.
    pub fn latest_attestation(&self, member: &MemberId) -> Option<i64> {
        self.latest.get(member).copied()
    }

    /// Compute humanness freshness for a member at the given time.
    ///
    /// Returns 1.0 if attested within the grace period, exponentially
    /// decaying after. Returns 0.0 if never attested.
    pub fn freshness_at(&self, member: &MemberId, now_millis: i64) -> f64 {
        match self.latest.get(member) {
            Some(&ts) => humanness_freshness(ts, now_millis),
            None => 0.0,
        }
    }

    /// Get all members with their latest attestation timestamps.
    pub fn all_latest(&self) -> &HashMap<MemberId, i64> {
        &self.latest
    }

    /// Get the event log (for inspection/debugging).
    pub fn events(&self) -> &[HumannessEvent] {
        &self.events
    }

    /// Rebuild derived state from the event log.
    pub fn rebuild_derived_state(&mut self) {
        self.latest.clear();

        for event in &self.events {
            match event {
                HumannessEvent::Attested(att) => {
                    let entry = self.latest.entry(att.subject).or_insert(0);
                    if att.timestamp_millis > *entry {
                        *entry = att.timestamp_millis;
                    }
                }
                HumannessEvent::ProofOfLife {
                    participants,
                    timestamp_millis,
                    ..
                } => {
                    for member in participants {
                        let entry = self.latest.entry(*member).or_insert(0);
                        if *timestamp_millis > *entry {
                            *entry = *timestamp_millis;
                        }
                    }
                }
            }
        }
    }

    /// Merge another document into this one (CRDT merge).
    ///
    /// Union of events with deduplication by exact equality.
    pub fn merge(&mut self, other: &HumannessDocument) {
        for event in &other.events {
            if !self.events.contains(event) {
                self.events.push(event.clone());
            }
        }
        self.rebuild_derived_state();
    }
}

/// Errors that can occur when validating a delegation chain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DelegationError {
    /// The delegation chain is empty but was expected to have links.
    EmptyChain,
    /// Chain connectivity broken: link N's delegate != link N+1's delegator.
    BrokenChain { link_index: usize },
    /// Levels are not descending (each link should go deeper in the hierarchy).
    LevelNotDescending { link_index: usize },
    /// A level was skipped (each step must be exactly one level deeper).
    LevelSkipped { link_index: usize },
    /// The attester does not match the final delegate in the chain.
    AttesterMismatch,
}

impl std::fmt::Display for DelegationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DelegationError::EmptyChain => write!(f, "delegation chain is empty"),
            DelegationError::BrokenChain { link_index } => {
                write!(f, "broken chain at link {link_index}: delegate != next delegator")
            }
            DelegationError::LevelNotDescending { link_index } => {
                write!(f, "level not descending at link {link_index}")
            }
            DelegationError::LevelSkipped { link_index } => {
                write!(f, "level skipped at link {link_index}")
            }
            DelegationError::AttesterMismatch => {
                write!(f, "attester does not match final delegate in chain")
            }
        }
    }
}

impl std::error::Error for DelegationError {}

/// Validate a delegation chain's structural integrity.
///
/// Checks:
/// 1. **Chain connectivity** — each link's delegate == next link's delegator
/// 2. **Level descent** — levels go Root -> Realm -> ... -> Individual
/// 3. **No level skipping** — each step is exactly one level deeper
/// 4. **Attester match** — attester matches the final delegate
///
/// An empty chain is valid (represents peer-to-peer attestation with no delegation).
///
/// NOTE: This cannot validate MemberId-to-temple mapping (that's subjective).
pub fn validate_delegation_chain(attestation: &HumannessAttestation) -> Result<(), DelegationError> {
    let chain = &attestation.delegation_chain;

    // Empty chain = peer-to-peer attestation, always valid
    if chain.is_empty() {
        return Ok(());
    }

    // Check that attester matches the final delegate
    if let Some(last) = chain.last() {
        if last.delegate != attestation.attester {
            return Err(DelegationError::AttesterMismatch);
        }
    }

    // Validate each consecutive pair of links
    for i in 0..chain.len() {
        // Check level descent: each link's level must be exactly one deeper than previous
        if i > 0 {
            let prev_level = chain[i - 1].level;
            let curr_level = chain[i].level;

            // Level must be strictly greater (deeper)
            if curr_level <= prev_level {
                return Err(DelegationError::LevelNotDescending { link_index: i });
            }

            // Level must be exactly one step deeper
            if curr_level.depth() != prev_level.depth() + 1 {
                return Err(DelegationError::LevelSkipped { link_index: i });
            }

            // Chain connectivity: previous delegate == current delegator
            if chain[i - 1].delegate != chain[i].delegator {
                return Err(DelegationError::BrokenChain { link_index: i });
            }
        }
    }

    Ok(())
}

impl HumannessDocument {
    /// Record a humanness attestation with optional chain validation.
    ///
    /// If `validate` is true, the delegation chain is validated before recording.
    /// If validation fails, the attestation is not recorded and an error is returned.
    /// If `validate` is false, this behaves identically to [`attest`](Self::attest).
    pub fn attest_validated(
        &mut self,
        attestation: HumannessAttestation,
        validate: bool,
    ) -> Result<(), DelegationError> {
        if validate {
            validate_delegation_chain(&attestation)?;
        }
        self.attest(attestation);
        Ok(())
    }
}

/// Compute humanness freshness from a timestamp.
///
/// - Returns 1.0 if within the 7-day grace period
/// - Returns `e^(-0.1 × (days - 7))` after the grace period
/// - Returns 0.0 if `last_attestation_millis` is 0 or in the future relative to now
pub fn humanness_freshness(last_attestation_millis: i64, now_millis: i64) -> f64 {
    if last_attestation_millis <= 0 || now_millis < last_attestation_millis {
        return 0.0;
    }

    let elapsed_millis = (now_millis - last_attestation_millis) as f64;
    let elapsed_days = elapsed_millis / (24.0 * 60.0 * 60.0 * 1000.0);

    if elapsed_days <= FRESHNESS_GRACE_DAYS {
        1.0
    } else {
        let excess_days = elapsed_days - FRESHNESS_GRACE_DAYS;
        (-FRESHNESS_DECAY_RATE * excess_days).exp()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member(n: u8) -> MemberId {
        [n; 32]
    }

    const DAY_MILLIS: i64 = 24 * 60 * 60 * 1000;

    #[test]
    fn test_freshness_within_grace_period() {
        let now = 1_000_000_000_000i64;
        let attested = now - 3 * DAY_MILLIS; // 3 days ago
        assert!((humanness_freshness(attested, now) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_freshness_at_grace_boundary() {
        let now = 1_000_000_000_000i64;
        let attested = now - 7 * DAY_MILLIS; // exactly 7 days
        assert!((humanness_freshness(attested, now) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_freshness_decay_after_grace() {
        let now = 1_000_000_000_000i64;
        let attested = now - 14 * DAY_MILLIS; // 14 days = 7 excess
        let expected = (-0.1 * 7.0_f64).exp(); // ~0.4966
        let actual = humanness_freshness(attested, now);
        assert!((actual - expected).abs() < 0.001);
    }

    #[test]
    fn test_freshness_30_days() {
        let now = 1_000_000_000_000i64;
        let attested = now - 30 * DAY_MILLIS; // 30 days = 23 excess
        let expected = (-0.1 * 23.0_f64).exp(); // ~0.100
        let actual = humanness_freshness(attested, now);
        assert!((actual - expected).abs() < 0.001);
    }

    #[test]
    fn test_freshness_never_attested() {
        assert_eq!(humanness_freshness(0, 1_000_000_000_000), 0.0);
    }

    #[test]
    fn test_freshness_future_attestation() {
        let now = 1_000_000_000_000i64;
        assert_eq!(humanness_freshness(now + DAY_MILLIS, now), 0.0);
    }

    #[test]
    fn test_attest_and_lookup() {
        let mut doc = HumannessDocument::new();
        let subject = member(1);
        let attester = member(2);
        let ts = 1_000_000_000_000i64;

        doc.attest(HumannessAttestation {
            subject,
            attester,
            delegation_chain: vec![],
            timestamp_millis: ts,
        });

        assert_eq!(doc.latest_attestation(&subject), Some(ts));
        assert!((doc.freshness_at(&subject, ts + DAY_MILLIS) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_proof_of_life_attests_all_participants() {
        let mut doc = HumannessDocument::new();
        let a = member(1);
        let b = member(2);
        let c = member(3);
        let ts = 1_000_000_000_000i64;

        doc.record_proof_of_life(vec![a, b, c], member(10), ts);

        assert_eq!(doc.latest_attestation(&a), Some(ts));
        assert_eq!(doc.latest_attestation(&b), Some(ts));
        assert_eq!(doc.latest_attestation(&c), Some(ts));
    }

    #[test]
    fn test_latest_attestation_wins() {
        let mut doc = HumannessDocument::new();
        let subject = member(1);
        let ts1 = 1_000_000_000_000i64;
        let ts2 = ts1 + 5 * DAY_MILLIS;

        doc.attest(HumannessAttestation {
            subject,
            attester: member(2),
            delegation_chain: vec![],
            timestamp_millis: ts1,
        });
        doc.attest(HumannessAttestation {
            subject,
            attester: member(3),
            delegation_chain: vec![],
            timestamp_millis: ts2,
        });

        assert_eq!(doc.latest_attestation(&subject), Some(ts2));
    }

    #[test]
    fn test_unattested_member_freshness_zero() {
        let doc = HumannessDocument::new();
        assert_eq!(doc.freshness_at(&member(99), 1_000_000_000_000), 0.0);
    }

    #[test]
    fn test_merge_deduplication() {
        let mut doc1 = HumannessDocument::new();
        let ts = 1_000_000_000_000i64;

        doc1.attest(HumannessAttestation {
            subject: member(1),
            attester: member(2),
            delegation_chain: vec![],
            timestamp_millis: ts,
        });

        let doc2 = doc1.clone();
        doc1.merge(&doc2);

        assert_eq!(doc1.events().len(), 1); // Not duplicated
    }

    #[test]
    fn test_merge_combines_events() {
        let mut doc1 = HumannessDocument::new();
        let mut doc2 = HumannessDocument::new();
        let ts = 1_000_000_000_000i64;

        doc1.attest(HumannessAttestation {
            subject: member(1),
            attester: member(2),
            delegation_chain: vec![],
            timestamp_millis: ts,
        });

        doc2.attest(HumannessAttestation {
            subject: member(3),
            attester: member(4),
            delegation_chain: vec![],
            timestamp_millis: ts + DAY_MILLIS,
        });

        doc1.merge(&doc2);

        assert_eq!(doc1.events().len(), 2);
        assert!(doc1.latest_attestation(&member(1)).is_some());
        assert!(doc1.latest_attestation(&member(3)).is_some());
    }

    #[test]
    fn test_rebuild_derived_state() {
        let mut doc = HumannessDocument::new();
        let ts = 1_000_000_000_000i64;

        doc.attest(HumannessAttestation {
            subject: member(1),
            attester: member(2),
            delegation_chain: vec![],
            timestamp_millis: ts,
        });
        doc.record_proof_of_life(vec![member(3), member(4)], member(5), ts + DAY_MILLIS);

        // Clear and rebuild
        doc.latest.clear();
        doc.rebuild_derived_state();

        assert_eq!(doc.latest_attestation(&member(1)), Some(ts));
        assert_eq!(doc.latest_attestation(&member(3)), Some(ts + DAY_MILLIS));
        assert_eq!(doc.latest_attestation(&member(4)), Some(ts + DAY_MILLIS));
    }

    #[test]
    fn test_delegation_chain_in_attestation() {
        let mut doc = HumannessDocument::new();
        let ts = 1_000_000_000_000i64;

        let chain = vec![
            Delegation {
                delegator: member(100), // Root
                delegate: member(101),  // Realm Temple
                level: BioregionalLevel::Realm,
                timestamp_millis: ts - 30 * DAY_MILLIS,
            },
            Delegation {
                delegator: member(101),
                delegate: member(102), // Ecoregion Temple
                level: BioregionalLevel::Ecoregion,
                timestamp_millis: ts - 20 * DAY_MILLIS,
            },
        ];

        doc.attest(HumannessAttestation {
            subject: member(1),
            attester: member(102),
            delegation_chain: chain.clone(),
            timestamp_millis: ts,
        });

        let events = doc.events();
        match &events[0] {
            HumannessEvent::Attested(att) => {
                assert_eq!(att.delegation_chain.len(), 2);
                assert_eq!(att.delegation_chain[0].level, BioregionalLevel::Realm);
                assert_eq!(att.delegation_chain[1].level, BioregionalLevel::Ecoregion);
            }
            _ => panic!("Expected Attested event"),
        }
    }

    // --- BioregionalLevel tests ---

    #[test]
    fn test_bioregional_level_depth() {
        assert_eq!(BioregionalLevel::Root.depth(), 0);
        assert_eq!(BioregionalLevel::Realm.depth(), 1);
        assert_eq!(BioregionalLevel::Subrealm.depth(), 2);
        assert_eq!(BioregionalLevel::Bioregion.depth(), 3);
        assert_eq!(BioregionalLevel::Ecoregion.depth(), 4);
        assert_eq!(BioregionalLevel::Individual.depth(), 5);
    }

    #[test]
    fn test_bioregional_level_ordering() {
        assert!(BioregionalLevel::Root < BioregionalLevel::Realm);
        assert!(BioregionalLevel::Realm < BioregionalLevel::Subrealm);
        assert!(BioregionalLevel::Subrealm < BioregionalLevel::Bioregion);
        assert!(BioregionalLevel::Bioregion < BioregionalLevel::Ecoregion);
        assert!(BioregionalLevel::Ecoregion < BioregionalLevel::Individual);
    }

    #[test]
    fn test_bioregional_level_parent() {
        assert_eq!(BioregionalLevel::Root.parent_level(), None);
        assert_eq!(BioregionalLevel::Realm.parent_level(), Some(BioregionalLevel::Root));
        assert_eq!(BioregionalLevel::Individual.parent_level(), Some(BioregionalLevel::Ecoregion));
    }

    // --- Delegation chain validation tests ---

    #[test]
    fn test_valid_full_delegation_chain() {
        let ts = 1_000_000_000_000i64;

        let chain = vec![
            Delegation {
                delegator: member(100),
                delegate: member(101),
                level: BioregionalLevel::Realm,
                timestamp_millis: ts - 50 * DAY_MILLIS,
            },
            Delegation {
                delegator: member(101),
                delegate: member(102),
                level: BioregionalLevel::Subrealm,
                timestamp_millis: ts - 40 * DAY_MILLIS,
            },
            Delegation {
                delegator: member(102),
                delegate: member(103),
                level: BioregionalLevel::Bioregion,
                timestamp_millis: ts - 30 * DAY_MILLIS,
            },
            Delegation {
                delegator: member(103),
                delegate: member(104),
                level: BioregionalLevel::Ecoregion,
                timestamp_millis: ts - 20 * DAY_MILLIS,
            },
            Delegation {
                delegator: member(104),
                delegate: member(105),
                level: BioregionalLevel::Individual,
                timestamp_millis: ts - 10 * DAY_MILLIS,
            },
        ];

        let attestation = HumannessAttestation {
            subject: member(1),
            attester: member(105),
            delegation_chain: chain,
            timestamp_millis: ts,
        };

        assert_eq!(validate_delegation_chain(&attestation), Ok(()));
    }

    #[test]
    fn test_empty_chain_valid() {
        let ts = 1_000_000_000_000i64;
        let attestation = HumannessAttestation {
            subject: member(1),
            attester: member(2),
            delegation_chain: vec![],
            timestamp_millis: ts,
        };
        assert_eq!(validate_delegation_chain(&attestation), Ok(()));
    }

    #[test]
    fn test_broken_chain_rejected() {
        let ts = 1_000_000_000_000i64;
        let chain = vec![
            Delegation {
                delegator: member(100),
                delegate: member(101),
                level: BioregionalLevel::Realm,
                timestamp_millis: ts,
            },
            Delegation {
                delegator: member(199), // WRONG: should be member(101)
                delegate: member(102),
                level: BioregionalLevel::Subrealm,
                timestamp_millis: ts,
            },
        ];

        let attestation = HumannessAttestation {
            subject: member(1),
            attester: member(102),
            delegation_chain: chain,
            timestamp_millis: ts,
        };

        assert_eq!(
            validate_delegation_chain(&attestation),
            Err(DelegationError::BrokenChain { link_index: 1 })
        );
    }

    #[test]
    fn test_level_not_descending_rejected() {
        let ts = 1_000_000_000_000i64;
        let chain = vec![
            Delegation {
                delegator: member(100),
                delegate: member(101),
                level: BioregionalLevel::Subrealm,
                timestamp_millis: ts,
            },
            Delegation {
                delegator: member(101),
                delegate: member(102),
                level: BioregionalLevel::Realm, // WRONG: going up
                timestamp_millis: ts,
            },
        ];

        let attestation = HumannessAttestation {
            subject: member(1),
            attester: member(102),
            delegation_chain: chain,
            timestamp_millis: ts,
        };

        assert_eq!(
            validate_delegation_chain(&attestation),
            Err(DelegationError::LevelNotDescending { link_index: 1 })
        );
    }

    #[test]
    fn test_level_skipped_rejected() {
        let ts = 1_000_000_000_000i64;
        let chain = vec![
            Delegation {
                delegator: member(100),
                delegate: member(101),
                level: BioregionalLevel::Realm,
                timestamp_millis: ts,
            },
            Delegation {
                delegator: member(101),
                delegate: member(102),
                level: BioregionalLevel::Bioregion, // WRONG: skipped Subrealm
                timestamp_millis: ts,
            },
        ];

        let attestation = HumannessAttestation {
            subject: member(1),
            attester: member(102),
            delegation_chain: chain,
            timestamp_millis: ts,
        };

        assert_eq!(
            validate_delegation_chain(&attestation),
            Err(DelegationError::LevelSkipped { link_index: 1 })
        );
    }

    #[test]
    fn test_attester_mismatch_rejected() {
        let ts = 1_000_000_000_000i64;
        let chain = vec![
            Delegation {
                delegator: member(100),
                delegate: member(101),
                level: BioregionalLevel::Realm,
                timestamp_millis: ts,
            },
        ];

        let attestation = HumannessAttestation {
            subject: member(1),
            attester: member(199), // WRONG: doesn't match delegate member(101)
            delegation_chain: chain,
            timestamp_millis: ts,
        };

        assert_eq!(
            validate_delegation_chain(&attestation),
            Err(DelegationError::AttesterMismatch)
        );
    }

    #[test]
    fn test_attest_validated_accepts_valid_chain() {
        let mut doc = HumannessDocument::new();
        let ts = 1_000_000_000_000i64;

        let chain = vec![
            Delegation {
                delegator: member(100),
                delegate: member(101),
                level: BioregionalLevel::Realm,
                timestamp_millis: ts,
            },
            Delegation {
                delegator: member(101),
                delegate: member(102),
                level: BioregionalLevel::Subrealm,
                timestamp_millis: ts,
            },
        ];

        let attestation = HumannessAttestation {
            subject: member(1),
            attester: member(102),
            delegation_chain: chain,
            timestamp_millis: ts,
        };

        assert!(doc.attest_validated(attestation, true).is_ok());
        assert_eq!(doc.latest_attestation(&member(1)), Some(ts));
    }

    #[test]
    fn test_attest_validated_rejects_invalid_chain() {
        let mut doc = HumannessDocument::new();
        let ts = 1_000_000_000_000i64;

        let chain = vec![
            Delegation {
                delegator: member(100),
                delegate: member(101),
                level: BioregionalLevel::Realm,
                timestamp_millis: ts,
            },
            Delegation {
                delegator: member(101),
                delegate: member(102),
                level: BioregionalLevel::Bioregion, // Skipped
                timestamp_millis: ts,
            },
        ];

        let attestation = HumannessAttestation {
            subject: member(1),
            attester: member(102),
            delegation_chain: chain,
            timestamp_millis: ts,
        };

        assert!(doc.attest_validated(attestation, true).is_err());
        assert_eq!(doc.latest_attestation(&member(1)), None); // Not recorded
    }
}
