//! # Attention tracking
//!
//! Locally-conservative attention log with PQ-signed, hash-chained events.
//!
//! Each player maintains a personal append-only chain of [`AttentionSwitchEvent`]s.
//! Events record navigation between artifacts (intentions, stories, etc.) and
//! form the basis for heat computation and token minting.
//!
//! ## Key types
//!
//! - [`AttentionSwitchEvent`]: A single navigation event (hash-chained, PQ-signable).
//! - [`AttentionLog`]: Per-player chain state + backing store.
//! - [`AttentionValue`]: Computed attention value for an artifact.
//! - [`DwellWindow`]: A continuous window of attention on a single artifact.
//!
//! ## Submodules
//!
//! - [`validate`]: Chain integrity and signature verification.
//! - [`fraud`]: Fraud proof construction and verification.

pub mod fraud;
pub mod validate;

use serde::{Deserialize, Serialize};

use crate::artifact::{ArtifactId, PlayerId};
use crate::error::VaultError;
use crate::store::AttentionStore;

type Result<T> = std::result::Result<T, VaultError>;

// ---------------------------------------------------------------------------
// AttentionSwitchEvent
// ---------------------------------------------------------------------------

/// A single attention-switch event in a player's hash-chained log.
///
/// Events are append-only. Each event references the hash of the previous
/// event (`prev`), forming a tamper-evident chain per author. The `sig` field
/// holds PQ signature bytes (empty `Vec` when unsigned).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttentionSwitchEvent {
    /// Protocol version (currently 1).
    pub version: u16,
    /// Author of this event.
    pub author: PlayerId,
    /// Monotonically increasing sequence number per author.
    pub seq: u64,
    /// Wall-clock time in milliseconds.
    pub wall_time_ms: i64,
    /// Artifact attention is leaving (None for genesis).
    pub from: Option<ArtifactId>,
    /// Artifact attention is moving to (None for farewell).
    pub to: Option<ArtifactId>,
    /// BLAKE3 hash of the previous event in this author's chain (zeros for genesis).
    pub prev: [u8; 32],
    /// PQ signature bytes (empty Vec when unsigned).
    pub sig: Vec<u8>,
}

impl AttentionSwitchEvent {
    /// Create an unsigned event with all fields except `sig` populated.
    pub fn new(
        author: PlayerId,
        seq: u64,
        wall_time_ms: i64,
        from: Option<ArtifactId>,
        to: Option<ArtifactId>,
        prev: [u8; 32],
    ) -> Self {
        Self {
            version: 1,
            author,
            seq,
            wall_time_ms,
            from,
            to,
            prev,
            sig: Vec::new(),
        }
    }

    /// Canonical bytes for signing (all fields except `sig`, serialized with postcard).
    pub fn signable_bytes(&self) -> Vec<u8> {
        #[derive(Serialize)]
        struct SignableFields<'a> {
            version: u16,
            author: &'a PlayerId,
            seq: u64,
            wall_time_ms: i64,
            from: &'a Option<ArtifactId>,
            to: &'a Option<ArtifactId>,
            prev: &'a [u8; 32],
        }
        let fields = SignableFields {
            version: self.version,
            author: &self.author,
            seq: self.seq,
            wall_time_ms: self.wall_time_ms,
            from: &self.from,
            to: &self.to,
            prev: &self.prev,
        };
        postcard::to_allocvec(&fields).expect("serialization cannot fail for fixed-schema types")
    }

    /// BLAKE3 hash of the full event (including sig), used as chain link.
    pub fn event_hash(&self) -> [u8; 32] {
        let encoded =
            postcard::to_allocvec(self).expect("serialization cannot fail for fixed-schema types");
        *blake3::hash(&encoded).as_bytes()
    }

    /// Sign this event with a PQ identity, filling the `sig` field.
    pub fn sign(&mut self, identity: &indras_crypto::PQIdentity) {
        let msg = self.signable_bytes();
        let signature = identity.sign(&msg);
        self.sig = signature.to_bytes().to_vec();
    }

    /// Verify the signature against a public key.
    pub fn verify_signature(&self, public_key: &indras_crypto::PQPublicIdentity) -> bool {
        if self.sig.is_empty() {
            return false;
        }
        let Ok(signature) = indras_crypto::PQSignature::from_bytes(self.sig.clone()) else {
            return false;
        };
        let msg = self.signable_bytes();
        public_key.verify(&msg, &signature)
    }

    /// Whether this event has been signed.
    pub fn is_signed(&self) -> bool {
        !self.sig.is_empty()
    }

    /// Whether this is a genesis event (seq 0, prev all zeros, from is None).
    pub fn is_genesis(&self) -> bool {
        self.seq == 0 && self.prev == [0u8; 32] && self.from.is_none()
    }
}

// ---------------------------------------------------------------------------
// AttentionLog
// ---------------------------------------------------------------------------

/// Per-player attention log with chain state tracking.
///
/// Wraps an [`AttentionStore`] and maintains the current focus, next sequence
/// number, and latest event hash for building the hash chain.
pub struct AttentionLog<S: AttentionStore> {
    /// The player this log belongs to.
    pub player: PlayerId,
    /// Current focus (what the player is attending to).
    current_focus: Option<ArtifactId>,
    /// Next sequence number for this author's chain.
    next_seq: u64,
    /// Hash of the latest event (zeros if no events yet).
    latest_hash: [u8; 32],
    /// Backing store.
    store: S,
}

impl<S: AttentionStore> AttentionLog<S> {
    /// Create a new attention log for a player.
    pub fn new(player: PlayerId, store: S) -> Self {
        Self {
            player,
            current_focus: None,
            next_seq: 0,
            latest_hash: [0u8; 32],
            store,
        }
    }

    /// Navigate to an artifact, recording an attention switch event.
    pub fn navigate_to(&mut self, artifact_id: ArtifactId, now: i64) -> Result<()> {
        let from = self.current_focus;
        let event = AttentionSwitchEvent::new(
            self.player,
            self.next_seq,
            now,
            from,
            Some(artifact_id),
            self.latest_hash,
        );
        let hash = event.event_hash();
        self.store.append_event(event)?;
        self.current_focus = Some(artifact_id);
        self.next_seq += 1;
        self.latest_hash = hash;
        Ok(())
    }

    /// Navigate back to a parent artifact.
    pub fn navigate_back(&mut self, parent_id: ArtifactId, now: i64) -> Result<()> {
        let from = self.current_focus;
        let event = AttentionSwitchEvent::new(
            self.player,
            self.next_seq,
            now,
            from,
            Some(parent_id),
            self.latest_hash,
        );
        let hash = event.event_hash();
        self.store.append_event(event)?;
        self.current_focus = Some(parent_id);
        self.next_seq += 1;
        self.latest_hash = hash;
        Ok(())
    }

    /// End the session, recording a farewell event (to: None).
    pub fn end_session(&mut self, now: i64) -> Result<()> {
        let from = self.current_focus.take();
        let event = AttentionSwitchEvent::new(
            self.player,
            self.next_seq,
            now,
            from,
            None,
            self.latest_hash,
        );
        let hash = event.event_hash();
        self.store.append_event(event)?;
        self.next_seq += 1;
        self.latest_hash = hash;
        Ok(())
    }

    /// What the player is currently attending to.
    pub fn current_focus(&self) -> Option<&ArtifactId> {
        self.current_focus.as_ref()
    }

    /// Get all events for this player from the store.
    pub fn events(&self) -> Result<Vec<AttentionSwitchEvent>> {
        self.store.events(&self.player)
    }

    /// Check integrity of a peer's log against our replica.
    pub fn check_peer_integrity(
        &self,
        peer: &PlayerId,
        their_events: &[AttentionSwitchEvent],
    ) -> crate::store::IntegrityResult {
        self.store.check_integrity(peer, their_events)
    }
}

// ---------------------------------------------------------------------------
// AttentionValue / DwellWindow
// ---------------------------------------------------------------------------

/// Computed attention value for an artifact (perspectival to the computing player).
#[derive(Clone, Debug, PartialEq)]
pub struct AttentionValue {
    /// Total dwell time across all peers, in milliseconds.
    pub total_dwell_ms: i64,
    /// Heat value (0.0 to 1.0), decaying over time.
    pub heat: f32,
    /// Dwell time from the local player only, in milliseconds.
    pub self_dwell_ms: i64,
}

/// A continuous window where a player attended to a specific artifact.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DwellWindow {
    /// The player who attended.
    pub player: PlayerId,
    /// The artifact that was attended to.
    pub artifact_id: ArtifactId,
    /// Start time in milliseconds.
    pub start_timestamp: i64,
    /// End time in milliseconds.
    pub end_timestamp: i64,
    /// Duration in milliseconds.
    pub duration_ms: i64,
}

// ---------------------------------------------------------------------------
// Free functions
// ---------------------------------------------------------------------------

/// Compute total dwell time (ms) for `artifact_id` from a slice of events.
///
/// Scans consecutive event pairs: if an event switches *to* `artifact_id`
/// and the next event switches *from* it, the gap is counted as dwell time.
/// If the last event switches *to* `artifact_id` with no successor, `now`
/// is used as the end time.
pub fn compute_dwell_time(events: &[AttentionSwitchEvent], artifact_id: &ArtifactId, now: i64) -> i64 {
    let mut total: i64 = 0;
    let mut i = 0;
    while i < events.len() {
        let e = &events[i];
        if e.to.as_ref() == Some(artifact_id) {
            let start = e.wall_time_ms;
            // Look for the next event that leaves this artifact
            if i + 1 < events.len() {
                let next = &events[i + 1];
                if next.from.as_ref() == Some(artifact_id) {
                    total += next.wall_time_ms - start;
                }
            } else {
                // Still focused — use now as end time
                total += now - start;
            }
        }
        i += 1;
    }
    total
}

/// Compute the full attention value for an artifact given peer logs and audience.
///
/// Heat decays exponentially: each peer's dwell time contributes
/// `dwell_ms / (dwell_ms + 60_000)` scaled by recency, then averaged across
/// the audience.
pub fn compute_heat(
    artifact_id: &ArtifactId,
    peer_logs: &[(PlayerId, &[AttentionSwitchEvent])],
    audience: &[PlayerId],
    now: i64,
) -> AttentionValue {
    if audience.is_empty() {
        return AttentionValue {
            total_dwell_ms: 0,
            heat: 0.0,
            self_dwell_ms: 0,
        };
    }

    let mut total_dwell: i64 = 0;
    let mut self_dwell: i64 = 0;
    let mut heat_sum: f32 = 0.0;

    for &member in audience {
        let member_events: Vec<&AttentionSwitchEvent> = peer_logs
            .iter()
            .filter(|(id, _)| *id == member)
            .flat_map(|(_, events)| events.iter())
            .filter(|e| e.author == member)
            .collect();

        let owned: Vec<AttentionSwitchEvent> = member_events.iter().map(|e| (*e).clone()).collect();
        let dwell = compute_dwell_time(&owned, artifact_id, now);
        total_dwell += dwell;

        // Heat contribution: saturating curve + recency decay
        if dwell > 0 {
            let saturation = dwell as f32 / (dwell as f32 + 60_000.0);
            // Find most recent event touching this artifact
            let last_touch = owned
                .iter()
                .filter(|e| e.to.as_ref() == Some(artifact_id) || e.from.as_ref() == Some(artifact_id))
                .map(|e| e.wall_time_ms)
                .max()
                .unwrap_or(0);
            let age_ms = (now - last_touch).max(0) as f32;
            let recency = (-age_ms / 300_000.0).exp(); // 5-minute half-life
            heat_sum += saturation * recency;
        }

        // Track self-dwell separately (first audience member is conventionally self)
        if member == audience[0] {
            self_dwell = dwell;
        }
    }

    let heat = (heat_sum / audience.len() as f32).clamp(0.0, 1.0);

    AttentionValue {
        total_dwell_ms: total_dwell,
        heat,
        self_dwell_ms: self_dwell,
    }
}

/// Extract continuous dwell windows for a player on a specific artifact.
///
/// Returns a [`DwellWindow`] for each contiguous period where the player
/// was focused on `artifact_id`.
pub fn extract_dwell_windows(
    player: PlayerId,
    artifact_id: &ArtifactId,
    events: &[AttentionSwitchEvent],
) -> Vec<DwellWindow> {
    let mut windows = Vec::new();
    let player_events: Vec<&AttentionSwitchEvent> = events
        .iter()
        .filter(|e| e.author == player)
        .collect();

    let mut i = 0;
    while i < player_events.len() {
        let e = player_events[i];
        if e.to.as_ref() == Some(artifact_id) {
            let start = e.wall_time_ms;
            // Find the next event where focus leaves this artifact
            if i + 1 < player_events.len() {
                let next = player_events[i + 1];
                if next.from.as_ref() == Some(artifact_id) {
                    let end = next.wall_time_ms;
                    windows.push(DwellWindow {
                        player,
                        artifact_id: *artifact_id,
                        start_timestamp: start,
                        end_timestamp: end,
                        duration_ms: end - start,
                    });
                }
            }
        }
        i += 1;
    }
    windows
}
