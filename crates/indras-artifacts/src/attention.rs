use serde::{Deserialize, Serialize};

use crate::artifact::{ArtifactId, PlayerId};
use crate::store::{AttentionStore, IntegrityResult};
use crate::error::VaultError;

type Result<T> = std::result::Result<T, VaultError>;

/// The only event in the system. Records a player shifting attention
/// from one artifact to another.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttentionSwitchEvent {
    pub player: PlayerId,
    pub from: Option<ArtifactId>,
    pub to: Option<ArtifactId>,
    pub timestamp: i64,
}

/// High-level wrapper over an AttentionStore for a single player.
/// Navigation IS attention — there is no separate give_attention() API.
pub struct AttentionLog<S: AttentionStore> {
    pub player: PlayerId,
    current_focus: Option<ArtifactId>,
    store: S,
}

impl<S: AttentionStore> AttentionLog<S> {
    pub fn new(player: PlayerId, store: S) -> Self {
        Self {
            player,
            current_focus: None,
            store,
        }
    }

    /// Navigate to an artifact. This IS the attention event.
    pub fn navigate_to(&mut self, artifact_id: ArtifactId, now: i64) -> Result<()> {
        let event = AttentionSwitchEvent {
            player: self.player,
            from: self.current_focus.clone(),
            to: Some(artifact_id.clone()),
            timestamp: now,
        };
        self.store.append_event(event)?;
        self.current_focus = Some(artifact_id);
        Ok(())
    }

    /// Navigate back to a parent artifact (zoom out).
    pub fn navigate_back(&mut self, parent_id: ArtifactId, now: i64) -> Result<()> {
        self.navigate_to(parent_id, now)
    }

    /// End the current session (attention goes to None).
    pub fn end_session(&mut self, now: i64) -> Result<()> {
        let event = AttentionSwitchEvent {
            player: self.player,
            from: self.current_focus.clone(),
            to: None,
            timestamp: now,
        };
        self.store.append_event(event)?;
        self.current_focus = None;
        Ok(())
    }

    /// What the player is currently attending to.
    pub fn current_focus(&self) -> Option<&ArtifactId> {
        self.current_focus.as_ref()
    }

    /// Total dwell time (millis) on a specific artifact, computed from consecutive switches.
    pub fn dwell_time(&self, artifact_id: &ArtifactId) -> Result<u64> {
        let events = self.store.events(&self.player)?;
        Ok(compute_dwell_time(artifact_id, &events))
    }

    /// All artifacts ranked by dwell time.
    pub fn dwell_times(&self) -> Result<Vec<(ArtifactId, u64)>> {
        let events = self.store.events(&self.player)?;
        let mut totals = std::collections::HashMap::<ArtifactId, u64>::new();

        for window in events.windows(2) {
            if let Some(ref to_id) = window[0].to {
                let dt = (window[1].timestamp - window[0].timestamp).max(0) as u64;
                *totals.entry(to_id.clone()).or_default() += dt;
            }
        }

        let mut result: Vec<_> = totals.into_iter().collect();
        result.sort_by(|a, b| b.1.cmp(&a.1));
        Ok(result)
    }

    /// Get events since a timestamp (for sync).
    pub fn events_since(&self, since: i64) -> Result<Vec<AttentionSwitchEvent>> {
        self.store.events_since(&self.player, since)
    }

    /// Get all events.
    pub fn events(&self) -> Result<Vec<AttentionSwitchEvent>> {
        self.store.events(&self.player)
    }

    /// Check integrity of a peer's log against our stored replica.
    pub fn check_peer_integrity(
        &self,
        peer: &PlayerId,
        their_events: &[AttentionSwitchEvent],
    ) -> IntegrityResult {
        self.store.check_integrity(peer, their_events)
    }

    /// Ingest a peer's attention log (read-only replica).
    pub fn ingest_peer_log(
        &mut self,
        peer: PlayerId,
        events: Vec<AttentionSwitchEvent>,
    ) -> Result<()> {
        self.store.ingest_peer_log(peer, events)
    }

    /// Get a mutable reference to the underlying store.
    pub fn store_mut(&mut self) -> &mut S {
        &mut self.store
    }

    /// Get a reference to the underlying store.
    pub fn store(&self) -> &S {
        &self.store
    }
}

/// Compute dwell time on a specific artifact from a sequence of events.
fn compute_dwell_time(artifact_id: &ArtifactId, events: &[AttentionSwitchEvent]) -> u64 {
    let mut total = 0u64;
    for window in events.windows(2) {
        if window[0].to.as_ref() == Some(artifact_id) {
            let dt = (window[1].timestamp - window[0].timestamp).max(0) as u64;
            total += dt;
        }
    }
    total
}

/// Derived attention value for an artifact, from one player's perspective.
#[derive(Clone, Debug, PartialEq)]
pub struct AttentionValue {
    pub artifact_id: ArtifactId,
    pub unique_peers: usize,
    pub total_dwell_millis: u64,
    /// Normalized 0.0 (cold) to 1.0 (hot). UI-ready as CSS `--heat`.
    pub heat: f32,
}

/// Compute perspectival heat for an artifact.
///
/// - `artifact_id`: the artifact to compute heat for
/// - `peer_logs`: attention logs from mutual peers (player_id -> events)
/// - `audience`: the artifact's audience (only peers in the audience count)
/// - `now`: current timestamp for recency weighting
pub fn compute_heat(
    artifact_id: &ArtifactId,
    peer_logs: &[(PlayerId, &[AttentionSwitchEvent])],
    audience: &[PlayerId],
    now: i64,
) -> AttentionValue {
    let mut unique_peers = 0usize;
    let mut total_dwell: u64 = 0;
    let mut recency_weighted_score: f64 = 0.0;

    // Half-life for recency weighting: 1 hour in millis
    const HALF_LIFE_MS: f64 = 3_600_000.0;

    for (peer_id, events) in peer_logs {
        // Only count peers in the audience
        if !audience.contains(peer_id) {
            continue;
        }

        let dwell = compute_dwell_time(artifact_id, events);
        if dwell == 0 {
            continue;
        }

        unique_peers += 1;
        total_dwell += dwell;

        // Find the most recent event involving this artifact for recency weighting
        let most_recent = events
            .iter()
            .filter(|e| e.to.as_ref() == Some(artifact_id) || e.from.as_ref() == Some(artifact_id))
            .map(|e| e.timestamp)
            .max()
            .unwrap_or(0);

        let age_ms = (now - most_recent).max(0) as f64;
        let recency_factor = (-age_ms * (2.0_f64.ln()) / HALF_LIFE_MS).exp();

        recency_weighted_score += (dwell as f64) * recency_factor;
    }

    // Normalize heat to 0.0–1.0 using a sigmoid-like curve.
    // Score of ~60000ms (1 minute of recent peer attention) maps to ~0.5 heat.
    const SCALE: f64 = 60_000.0;
    let heat = if recency_weighted_score > 0.0 {
        let normalized = recency_weighted_score / (recency_weighted_score + SCALE);
        (normalized as f32).clamp(0.0, 1.0)
    } else {
        0.0
    };

    AttentionValue {
        artifact_id: artifact_id.clone(),
        unique_peers,
        total_dwell_millis: total_dwell,
        heat,
    }
}
