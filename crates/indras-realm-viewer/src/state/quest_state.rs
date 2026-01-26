//! Quest tracking state
//!
//! Tracks quests with proof-of-service claims.

use std::collections::HashMap;

use crate::events::StreamEvent;

/// Quest status in the lifecycle
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum QuestStatus {
    #[default]
    Open,
    Claimed,
    Verified,
    Completed,
}

impl QuestStatus {
    pub fn display_name(&self) -> &'static str {
        match self {
            QuestStatus::Open => "Open",
            QuestStatus::Claimed => "Claimed",
            QuestStatus::Verified => "Verified",
            QuestStatus::Completed => "Done",
        }
    }

    pub fn css_class(&self) -> &'static str {
        match self {
            QuestStatus::Open => "status-open",
            QuestStatus::Claimed => "status-claimed",
            QuestStatus::Verified => "status-verified",
            QuestStatus::Completed => "status-completed",
        }
    }
}

/// A claim on a quest
#[derive(Clone, Debug, PartialEq)]
pub struct ClaimInfo {
    pub claimant: String,
    pub proof_artifact: Option<String>,
    pub verified: bool,
    pub submitted_at_tick: u32,
    pub verified_at_tick: Option<u32>,
}

/// Information about a quest
#[derive(Clone, Debug, PartialEq)]
pub struct QuestInfo {
    pub quest_id: String,
    pub realm_id: String,
    pub title: String,
    pub creator: String,
    pub claims: Vec<ClaimInfo>,
    pub status: QuestStatus,
    pub created_at_tick: u32,
    pub completed_at_tick: Option<u32>,
}

impl QuestInfo {
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
        if self.status == QuestStatus::Completed {
            return;
        }

        if self.verified_claims() > 0 {
            self.status = QuestStatus::Verified;
        } else if !self.claims.is_empty() {
            self.status = QuestStatus::Claimed;
        } else {
            self.status = QuestStatus::Open;
        }
    }
}

/// Quest tracking state
#[derive(Clone, Debug, Default)]
pub struct QuestState {
    /// All quests by ID
    pub quests: HashMap<String, QuestInfo>,
    /// Quests by realm
    pub by_realm: HashMap<String, Vec<String>>,
    /// Selected realm filter (None = all)
    pub selected_realm: Option<String>,
    /// Selected quest for details
    pub selected_quest: Option<String>,
}

impl QuestState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Process a quest-related event
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
                let quest = QuestInfo {
                    quest_id: quest_id.clone(),
                    realm_id: realm_id.clone(),
                    title: if title.is_empty() {
                        format!("Quest {}", &quest_id[..8.min(quest_id.len())])
                    } else {
                        title.clone()
                    },
                    creator: creator.clone(),
                    claims: Vec::new(),
                    status: QuestStatus::Open,
                    created_at_tick: *tick,
                    completed_at_tick: None,
                };

                self.quests.insert(quest_id.clone(), quest);
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
                if let Some(quest) = self.quests.get_mut(quest_id) {
                    quest.claims.push(ClaimInfo {
                        claimant: claimant.clone(),
                        proof_artifact: proof_artifact.clone(),
                        verified: false,
                        submitted_at_tick: *tick,
                        verified_at_tick: None,
                    });
                    quest.update_status();
                }
            }

            StreamEvent::QuestClaimVerified {
                tick,
                quest_id,
                claim_index,
                ..
            } => {
                if let Some(quest) = self.quests.get_mut(quest_id) {
                    if let Some(claim) = quest.claims.get_mut(*claim_index) {
                        claim.verified = true;
                        claim.verified_at_tick = Some(*tick);
                    }
                    quest.update_status();
                }
            }

            StreamEvent::QuestCompleted {
                tick, quest_id, ..
            } => {
                if let Some(quest) = self.quests.get_mut(quest_id) {
                    quest.status = QuestStatus::Completed;
                    quest.completed_at_tick = Some(*tick);
                }
            }

            _ => {}
        }
    }

    /// Get quests by status
    pub fn quests_by_status(&self, status: QuestStatus) -> Vec<&QuestInfo> {
        self.quests
            .values()
            .filter(|q| {
                q.status == status
                    && self
                        .selected_realm
                        .as_ref()
                        .map(|r| &q.realm_id == r)
                        .unwrap_or(true)
            })
            .collect()
    }

    /// Get all unique realm IDs that have quests
    pub fn realms_with_quests(&self) -> Vec<&String> {
        self.by_realm.keys().collect()
    }

    /// Get quest count by status
    pub fn count_by_status(&self, status: QuestStatus) -> usize {
        self.quests_by_status(status).len()
    }

    /// Get total quest count
    pub fn total_quests(&self) -> usize {
        self.quests.len()
    }

    /// Get quests where member is creator or has claims
    pub fn quests_for_member(&self, member: &str) -> Vec<&QuestInfo> {
        self.quests
            .values()
            .filter(|q| {
                q.creator == member || q.claims.iter().any(|c| c.claimant == member)
            })
            .collect()
    }

    /// Get quests by status filtered to member (creator or claimant)
    pub fn quests_for_member_by_status(&self, member: &str, status: QuestStatus) -> Vec<&QuestInfo> {
        self.quests
            .values()
            .filter(|q| {
                q.status == status
                    && (q.creator == member || q.claims.iter().any(|c| c.claimant == member))
                    && self
                        .selected_realm
                        .as_ref()
                        .map(|r| &q.realm_id == r)
                        .unwrap_or(true)
            })
            .collect()
    }
}
