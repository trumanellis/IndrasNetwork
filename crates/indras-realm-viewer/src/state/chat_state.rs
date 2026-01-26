//! Chat and Blessing State
//!
//! Tracks realm chat messages, proof submissions, and blessings.

use crate::events::StreamEvent;
use std::collections::{HashMap, VecDeque};

/// Maximum chat messages to keep per realm.
const MAX_CHAT_MESSAGES: usize = 100;

/// A chat message in the realm.
#[derive(Clone, Debug, PartialEq)]
pub struct ChatMessage {
    pub tick: u32,
    pub member: String,
    pub content: String,
    pub message_type: ChatMessageType,
}

/// Types of chat messages.
#[derive(Clone, Debug, PartialEq)]
pub enum ChatMessageType {
    /// Regular text message.
    Text,
    /// Proof submission for a quest.
    ProofSubmitted {
        quest_id: String,
        quest_title: String,
        artifact_id: String,
        artifact_name: String,
    },
    /// Proof folder submitted for a quest.
    ProofFolderSubmitted {
        quest_id: String,
        folder_id: String,
        artifact_count: usize,
        narrative_preview: String,
    },
    /// Blessing given to a proof.
    BlessingGiven {
        quest_id: String,
        claimant: String,
        attention_millis: u64,
    },
}

/// Blessing information for a proof.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ProofBlessingInfo {
    pub quest_id: String,
    pub quest_title: String,
    pub claimant: String,
    pub artifact_id: String,
    pub artifact_name: String,
    pub tick: u32,
    /// Total blessed attention in milliseconds.
    pub total_blessed_millis: u64,
    /// Number of members who blessed.
    pub blesser_count: usize,
    /// Individual blessings.
    pub blessings: Vec<BlessingInfo>,
}

/// Individual blessing.
#[derive(Clone, Debug, PartialEq)]
pub struct BlessingInfo {
    pub blesser: String,
    pub attention_millis: u64,
    pub tick: u32,
}

/// State tracking for chat messages and blessings.
#[derive(Clone, Debug, Default)]
pub struct ChatState {
    /// Chat messages by realm (newest first).
    pub messages_by_realm: HashMap<String, VecDeque<ChatMessage>>,
    /// Global chat feed (newest first).
    pub global_messages: VecDeque<ChatMessage>,
    /// Proof blessings indexed by (quest_id, claimant).
    pub proof_blessings: HashMap<(String, String), ProofBlessingInfo>,
    /// Total message count.
    pub total_messages: usize,
    /// Total blessings.
    pub total_blessings: usize,
}

impl ChatState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Process a stream event.
    pub fn process_event(&mut self, event: &StreamEvent) {
        match event {
            StreamEvent::ChatMessage {
                tick,
                member,
                content,
                ..
            } => {
                let msg = ChatMessage {
                    tick: *tick,
                    member: member.clone(),
                    content: content.clone(),
                    message_type: ChatMessageType::Text,
                };
                self.add_global_message(msg);
            }

            StreamEvent::ProofSubmitted {
                tick,
                realm_id,
                quest_id,
                claimant,
                quest_title,
                artifact_id,
                artifact_name,
            } => {
                // Create proof blessing entry
                let key = (quest_id.clone(), claimant.clone());
                self.proof_blessings.insert(
                    key,
                    ProofBlessingInfo {
                        quest_id: quest_id.clone(),
                        quest_title: quest_title.clone(),
                        claimant: claimant.clone(),
                        artifact_id: artifact_id.clone(),
                        artifact_name: artifact_name.clone(),
                        tick: *tick,
                        total_blessed_millis: 0,
                        blesser_count: 0,
                        blessings: Vec::new(),
                    },
                );

                // Add chat message
                let msg = ChatMessage {
                    tick: *tick,
                    member: claimant.clone(),
                    content: format!("Submitted proof for {}", quest_title),
                    message_type: ChatMessageType::ProofSubmitted {
                        quest_id: quest_id.clone(),
                        quest_title: quest_title.clone(),
                        artifact_id: artifact_id.clone(),
                        artifact_name: artifact_name.clone(),
                    },
                };
                self.add_realm_message(realm_id, msg.clone());
                self.add_global_message(msg);
            }

            StreamEvent::BlessingGiven {
                tick,
                realm_id,
                quest_id,
                claimant,
                blesser,
                attention_millis,
                ..
            } => {
                // Update proof blessing info
                let key = (quest_id.clone(), claimant.clone());
                if let Some(info) = self.proof_blessings.get_mut(&key) {
                    info.total_blessed_millis += attention_millis;
                    info.blesser_count += 1;
                    info.blessings.push(BlessingInfo {
                        blesser: blesser.clone(),
                        attention_millis: *attention_millis,
                        tick: *tick,
                    });
                }

                self.total_blessings += 1;

                // Add chat message
                let msg = ChatMessage {
                    tick: *tick,
                    member: blesser.clone(),
                    content: format!(
                        "Blessed {} with {}",
                        claimant,
                        format_duration(*attention_millis)
                    ),
                    message_type: ChatMessageType::BlessingGiven {
                        quest_id: quest_id.clone(),
                        claimant: claimant.clone(),
                        attention_millis: *attention_millis,
                    },
                };
                self.add_realm_message(realm_id, msg.clone());
                self.add_global_message(msg);
            }

            StreamEvent::ProofFolderSubmitted {
                tick,
                realm_id,
                quest_id,
                claimant,
                folder_id,
                artifact_count,
                narrative_preview,
            } => {
                // Create proof blessing entry (similar to ProofSubmitted)
                let key = (quest_id.clone(), claimant.clone());
                self.proof_blessings.insert(
                    key,
                    ProofBlessingInfo {
                        quest_id: quest_id.clone(),
                        quest_title: String::new(), // Not available in folder event
                        claimant: claimant.clone(),
                        artifact_id: folder_id.clone(),
                        artifact_name: format!("Proof folder ({} files)", artifact_count),
                        tick: *tick,
                        total_blessed_millis: 0,
                        blesser_count: 0,
                        blessings: Vec::new(),
                    },
                );

                // Add chat message
                let content = if narrative_preview.is_empty() {
                    format!("Submitted proof folder ({} files)", artifact_count)
                } else {
                    format!("Submitted proof: {}", narrative_preview)
                };

                let msg = ChatMessage {
                    tick: *tick,
                    member: claimant.clone(),
                    content,
                    message_type: ChatMessageType::ProofFolderSubmitted {
                        quest_id: quest_id.clone(),
                        folder_id: folder_id.clone(),
                        artifact_count: *artifact_count,
                        narrative_preview: narrative_preview.clone(),
                    },
                };
                self.add_realm_message(realm_id, msg.clone());
                self.add_global_message(msg);
            }

            _ => {}
        }
    }

    fn add_realm_message(&mut self, realm_id: &str, msg: ChatMessage) {
        let messages = self
            .messages_by_realm
            .entry(realm_id.to_string())
            .or_default();
        messages.push_front(msg);
        while messages.len() > MAX_CHAT_MESSAGES {
            messages.pop_back();
        }
        self.total_messages += 1;
    }

    fn add_global_message(&mut self, msg: ChatMessage) {
        self.global_messages.push_front(msg);
        while self.global_messages.len() > MAX_CHAT_MESSAGES {
            self.global_messages.pop_back();
        }
    }

    /// Get recent messages across all realms (oldest first for chat display).
    pub fn recent_messages(&self, limit: usize) -> Vec<&ChatMessage> {
        // Take last N messages and return in chronological order (oldest first)
        // so newest messages appear at the bottom of the chat
        let msgs: Vec<_> = self.global_messages.iter().take(limit).collect();
        msgs.into_iter().rev().collect()
    }

    /// Get messages for a specific realm.
    pub fn messages_for_realm(&self, realm_id: &str) -> Vec<&ChatMessage> {
        self.messages_by_realm
            .get(realm_id)
            .map(|m| m.iter().collect())
            .unwrap_or_default()
    }

    /// Get blessing info for a proof.
    pub fn blessing_info(&self, quest_id: &str, claimant: &str) -> Option<&ProofBlessingInfo> {
        self.proof_blessings.get(&(quest_id.to_string(), claimant.to_string()))
    }

    /// Get all proofs sorted by tick (newest first).
    pub fn recent_proofs(&self, limit: usize) -> Vec<&ProofBlessingInfo> {
        let mut proofs: Vec<_> = self.proof_blessings.values().collect();
        proofs.sort_by(|a, b| b.tick.cmp(&a.tick));
        proofs.into_iter().take(limit).collect()
    }
}

/// Format milliseconds as human-readable duration.
fn format_duration(millis: u64) -> String {
    let seconds = millis / 1000;
    let minutes = seconds / 60;
    let hours = minutes / 60;

    if hours > 0 {
        let remaining_mins = minutes % 60;
        if remaining_mins > 0 {
            format!("{}h {}m", hours, remaining_mins)
        } else {
            format!("{}h", hours)
        }
    } else if minutes > 0 {
        format!("{}m", minutes)
    } else {
        format!("{}s", seconds)
    }
}
