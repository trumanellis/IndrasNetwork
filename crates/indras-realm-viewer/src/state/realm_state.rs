//! Realm tracking state
//!
//! Tracks realms and their membership.

use std::collections::{HashMap, HashSet};

use crate::events::StreamEvent;

/// Information about a realm
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RealmInfo {
    pub realm_id: String,
    pub members: HashSet<String>,
    pub quest_count: usize,
    pub created_at_tick: u32,
}

/// Realm tracking state
#[derive(Clone, Debug, Default)]
pub struct RealmState {
    /// All known realms
    pub realms: HashMap<String, RealmInfo>,
    /// All known members across all realms
    pub all_members: HashSet<String>,
}

impl RealmState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Process a realm-related event
    pub fn process_event(&mut self, event: &StreamEvent) {
        match event {
            StreamEvent::RealmCreated {
                tick,
                realm_id,
                members,
                member_count: _,
            } => {
                let member_list: HashSet<String> = members
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();

                // Track all members
                for m in &member_list {
                    self.all_members.insert(m.clone());
                }

                self.realms.insert(
                    realm_id.clone(),
                    RealmInfo {
                        realm_id: realm_id.clone(),
                        members: member_list,
                        quest_count: 0,
                        created_at_tick: *tick,
                    },
                );
            }

            StreamEvent::MemberJoined {
                realm_id, member, ..
            } => {
                self.all_members.insert(member.clone());
                if let Some(realm) = self.realms.get_mut(realm_id) {
                    realm.members.insert(member.clone());
                }
            }

            StreamEvent::MemberLeft {
                realm_id, member, ..
            } => {
                if let Some(realm) = self.realms.get_mut(realm_id) {
                    realm.members.remove(member);
                }
            }

            _ => {}
        }
    }

    /// Increment quest count for a realm
    pub fn increment_quest_count(&mut self, realm_id: &str) {
        if let Some(realm) = self.realms.get_mut(realm_id) {
            realm.quest_count += 1;
        }
    }

    /// Get realms sorted by member count (descending)
    pub fn realms_by_size(&self) -> Vec<&RealmInfo> {
        let mut realms: Vec<_> = self.realms.values().collect();
        realms.sort_by(|a, b| b.members.len().cmp(&a.members.len()));
        realms
    }

    /// Get total realm count
    pub fn realm_count(&self) -> usize {
        self.realms.len()
    }

    /// Get total unique member count
    pub fn member_count(&self) -> usize {
        self.all_members.len()
    }

    /// Get realms containing the specified member
    pub fn realms_for_member(&self, member: &str) -> Vec<&RealmInfo> {
        let mut realms: Vec<_> = self.realms.values()
            .filter(|r| r.members.contains(member))
            .collect();
        realms.sort_by(|a, b| b.members.len().cmp(&a.members.len()));
        realms
    }
}
