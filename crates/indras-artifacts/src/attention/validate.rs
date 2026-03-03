//! Validation logic for attention switch event chains.
//!
//! Enforces the locally-conservative invariants:
//! - Signature verification (PQ signatures)
//! - Sequential ordering (monotonic seq, hash-linked prev)
//! - Attention continuity (event.from == prior.to)

use crate::artifact::ArtifactId;
use crate::attention::AttentionSwitchEvent;
use indras_crypto::PQPublicIdentity;
use thiserror::Error;

/// Errors from validating an attention event or chain.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum ValidationError {
    /// Signature is missing or fails verification.
    #[error("invalid signature")]
    InvalidSignature,
    /// Sequence number is not the expected next value.
    #[error("expected seq {expected}, got {got}")]
    SequenceGap {
        /// Expected sequence number.
        expected: u64,
        /// Actual sequence number found.
        got: u64,
    },
    /// The `prev` hash does not match the hash of the prior event.
    #[error("prev hash mismatch at seq {seq}")]
    PrevHashMismatch {
        /// Sequence number where mismatch occurred.
        seq: u64,
    },
    /// The `from` field does not match the prior event's `to` field.
    #[error("attention continuity broken at seq {seq}: expected from={expected:?}, got from={got:?}")]
    AttentionContinuity {
        /// Sequence number where break occurred.
        seq: u64,
        /// Expected `from` value (prior event's `to`).
        expected: Option<ArtifactId>,
        /// Actual `from` value found.
        got: Option<ArtifactId>,
    },
    /// First event in chain does not satisfy genesis constraints.
    #[error("genesis event must have seq=0, prev=zeros, from=None")]
    InvalidGenesis,
    /// Chain contains no events.
    #[error("empty chain")]
    EmptyChain,
}

/// State tracked while validating an author's chain.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthorState {
    /// Latest validated sequence number.
    pub latest_seq: u64,
    /// Hash of the latest validated event.
    pub latest_hash: [u8; 32],
    /// Current attention target after latest event.
    pub current_attention: Option<ArtifactId>,
}

/// Validate a single event against the author's current state.
///
/// Checks:
/// 1. Signature (if `public_key` provided)
/// 2. `seq == latest_seq + 1`
/// 3. `prev == latest_hash`
/// 4. `from == current_attention` (attention continuity)
pub fn validate_event(
    event: &AttentionSwitchEvent,
    author_state: &AuthorState,
    public_key: Option<&PQPublicIdentity>,
) -> Result<(), ValidationError> {
    // 1. Verify signature if key provided
    if let Some(pk) = public_key {
        if !event.verify_signature(pk) {
            return Err(ValidationError::InvalidSignature);
        }
    }

    // 2. Check sequence number
    let expected_seq = author_state.latest_seq + 1;
    if event.seq != expected_seq {
        return Err(ValidationError::SequenceGap {
            expected: expected_seq,
            got: event.seq,
        });
    }

    // 3. Check prev hash
    if event.prev != author_state.latest_hash {
        return Err(ValidationError::PrevHashMismatch { seq: event.seq });
    }

    // 4. Check attention continuity
    if event.from != author_state.current_attention {
        return Err(ValidationError::AttentionContinuity {
            seq: event.seq,
            expected: author_state.current_attention,
            got: event.from,
        });
    }

    Ok(())
}

/// Validate a genesis event (first event in a chain).
///
/// A valid genesis has `seq=0`, `prev=all-zeros`, `from=None`.
pub fn validate_genesis(
    event: &AttentionSwitchEvent,
    public_key: Option<&PQPublicIdentity>,
) -> Result<(), ValidationError> {
    if let Some(pk) = public_key {
        if !event.verify_signature(pk) {
            return Err(ValidationError::InvalidSignature);
        }
    }

    if !event.is_genesis() {
        return Err(ValidationError::InvalidGenesis);
    }

    Ok(())
}

/// Validate an entire chain of events from a single author.
///
/// Returns the final [`AuthorState`] on success.
pub fn validate_chain(
    events: &[AttentionSwitchEvent],
    public_key: Option<&PQPublicIdentity>,
) -> Result<AuthorState, ValidationError> {
    if events.is_empty() {
        return Err(ValidationError::EmptyChain);
    }

    // Validate genesis
    let first = &events[0];
    validate_genesis(first, public_key)?;

    let mut state = AuthorState {
        latest_seq: 0,
        latest_hash: first.event_hash(),
        current_attention: first.to,
    };

    // Validate remaining events
    for event in &events[1..] {
        validate_event(event, &state, public_key)?;
        state.latest_seq = event.seq;
        state.latest_hash = event.event_hash();
        state.current_attention = event.to;
    }

    Ok(state)
}
