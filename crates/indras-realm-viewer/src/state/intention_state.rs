//! Intention tracking state
//!
//! Tracks intentions with proof-of-service claims.

use std::collections::HashMap;

use crate::events::StreamEvent;

/// Intention status in the lifecycle
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum IntentionStatus {
    #[default]
    Open,
    Claimed,
    Verified,
    Completed,
}

impl IntentionStatus {
    pub fn display_name(&self) -> &'static str {
        match self {
            IntentionStatus::Open => "Open",
            IntentionStatus::Claimed => "Claimed",
            IntentionStatus::Verified => "Verified",
            IntentionStatus::Completed => "Done",
        }
    }

    pub fn css_class(&self) -> &'static str {
        match self {
            IntentionStatus::Open => "status-open",
            IntentionStatus::Claimed => "status-claimed",
            IntentionStatus::Verified => "status-verified",
            IntentionStatus::Completed => "status-completed",
        }
    }
}

/// A claim on an intention
#[derive(Clone, Debug, PartialEq)]
pub struct ClaimInfo {
    pub claimant: String,
    pub proof_artifact: Option<String>,
    pub verified: bool,
    pub submitted_at_tick: u32,
    pub verified_at_tick: Option<u32>,
}

/// Information about an intention
#[derive(Clone, Debug, PartialEq)]
pub struct IntentionInfo {
    pub intention_id: String,
    pub realm_id: String,
    pub title: String,
    pub creator: String,
    pub claims: Vec<ClaimInfo>,
    pub status: IntentionStatus,
    pub created_at_tick: u32,
    pub completed_at_tick: Option<u32>,
}

impl IntentionInfo {
    /// Count of pending (unverified) claims
    pub fn pending_claims(&self) -> usize {
        self.claims.iter().filter(|c| !c.verified).count()
    }

    /// Count of verified claims
    pub fn verified_claims(&self) -> usize {
        self.claims.iter().filter(|c| c.verified).count()
    }

    /// Update status based on claims
    fn update_status(&mut self) {
        if self.status == IntentionStatus::Completed {
            return;
        }

        if self.verified_claims() > 0 {
            self.status = IntentionStatus::Verified;
        } else if !self.claims.is_empty() {
            self.status = IntentionStatus::Claimed;
        } else {
            self.status = IntentionStatus::Open;
        }
    }
}

/// Intention tracking state
#[derive(Clone, Debug, Default)]
pub struct IntentionState {
    /// All intentions by ID
    pub intentions: HashMap<String, IntentionInfo>,
    /// Intentions by realm
    pub by_realm: HashMap<String, Vec<String>>,
    /// Selected realm filter (None = all)
    pub selected_realm: Option<String>,
    /// Selected intention for details
    pub selected_intention: Option<String>,
}

impl IntentionState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Process an intention-related event
    pub fn process_event(&mut self, event: &StreamEvent) {
        match event {
            StreamEvent::QuestCreated {
                tick,
                realm_id,
                quest_id,
                creator,
                title,
                ..
            } => {
                let intention = IntentionInfo {
                    intention_id: quest_id.clone(),
                    realm_id: realm_id.clone(),
                    title: if title.is_empty() {
                        format!("Intention {}", &quest_id[..8.min(quest_id.len())])
                    } else {
                        title.clone()
                    },
                    creator: creator.clone(),
                    claims: Vec::new(),
                    status: IntentionStatus::Open,
                    created_at_tick: *tick,
                    completed_at_tick: None,
                };

                self.intentions.insert(quest_id.clone(), intention);
                self.by_realm
                    .entry(realm_id.clone())
                    .or_default()
                    .push(quest_id.clone());
            }

            StreamEvent::QuestClaimSubmitted {
                tick,
                quest_id,
                claimant,
                proof_artifact,
                ..
            } => {
                if let Some(intention) = self.intentions.get_mut(quest_id) {
                    intention.claims.push(ClaimInfo {
                        claimant: claimant.clone(),
                        proof_artifact: proof_artifact.clone(),
                        verified: false,
                        submitted_at_tick: *tick,
                        verified_at_tick: None,
                    });
                    intention.update_status();
                }
            }

            StreamEvent::QuestClaimVerified {
                tick,
                quest_id,
                claim_index,
                ..
            } => {
                if let Some(intention) = self.intentions.get_mut(quest_id) {
                    if let Some(claim) = intention.claims.get_mut(*claim_index) {
                        claim.verified = true;
                        claim.verified_at_tick = Some(*tick);
                    }
                    intention.update_status();
                }
            }

            StreamEvent::QuestCompleted {
                tick, quest_id, ..
            } => {
                if let Some(intention) = self.intentions.get_mut(quest_id) {
                    intention.status = IntentionStatus::Completed;
                    intention.completed_at_tick = Some(*tick);
                }
            }

            _ => {}
        }
    }

    /// Get intentions by status
    pub fn intentions_by_status(&self, status: IntentionStatus) -> Vec<&IntentionInfo> {
        self.intentions
            .values()
            .filter(|i| {
                i.status == status
                    && self
                        .selected_realm
                        .as_ref()
                        .map(|r| &i.realm_id == r)
                        .unwrap_or(true)
            })
            .collect()
    }

    /// Get all unique realm IDs that have intentions
    pub fn realms_with_intentions(&self) -> Vec<&String> {
        self.by_realm.keys().collect()
    }

    /// Get intention count by status
    pub fn count_by_status(&self, status: IntentionStatus) -> usize {
        self.intentions_by_status(status).len()
    }

    /// Get total intention count
    pub fn total_intentions(&self) -> usize {
        self.intentions.len()
    }

    /// Get intentions where member is creator or has claims
    pub fn intentions_for_member(&self, member: &str) -> Vec<&IntentionInfo> {
        self.intentions
            .values()
            .filter(|i| {
                i.creator == member || i.claims.iter().any(|c| c.claimant == member)
            })
            .collect()
    }

    /// Get intentions by status filtered to member (creator or claimant)
    pub fn intentions_for_member_by_status(&self, member: &str, status: IntentionStatus) -> Vec<&IntentionInfo> {
        self.intentions
            .values()
            .filter(|i| {
                i.status == status
                    && (i.creator == member || i.claims.iter().any(|c| c.claimant == member))
                    && self
                        .selected_realm
                        .as_ref()
                        .map(|r| &i.realm_id == r)
                        .unwrap_or(true)
            })
            .collect()
    }
}
