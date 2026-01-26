//! Contacts tracking state
//!
//! Tracks contact relationships between members.

use std::collections::{HashMap, HashSet};

use crate::events::StreamEvent;

/// Contacts tracking state
#[derive(Clone, Debug, Default)]
pub struct ContactsState {
    /// Contacts per member: member -> set of contacts
    pub contacts: HashMap<String, HashSet<String>>,
    /// Reverse lookup: member -> who has them as contact
    pub contacted_by: HashMap<String, HashSet<String>>,
    /// Selected member for details panel
    pub selected_member: Option<String>,
}

impl ContactsState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Process a contacts-related event
    pub fn process_event(&mut self, event: &StreamEvent) {
        match event {
            StreamEvent::ContactAdded {
                member, contact, ..
            } => {
                self.contacts
                    .entry(member.clone())
                    .or_default()
                    .insert(contact.clone());
                self.contacted_by
                    .entry(contact.clone())
                    .or_default()
                    .insert(member.clone());
            }

            StreamEvent::ContactRemoved {
                member, contact, ..
            } => {
                if let Some(contacts) = self.contacts.get_mut(member) {
                    contacts.remove(contact);
                }
                if let Some(contacted_by) = self.contacted_by.get_mut(contact) {
                    contacted_by.remove(member);
                }
            }

            _ => {}
        }
    }

    /// Get all members who appear in the contacts graph
    pub fn all_members(&self) -> HashSet<String> {
        let mut members = HashSet::new();
        for (member, contacts) in &self.contacts {
            members.insert(member.clone());
            for c in contacts {
                members.insert(c.clone());
            }
        }
        members
    }

    /// Get contacts for a member
    pub fn get_contacts(&self, member: &str) -> Vec<&String> {
        self.contacts
            .get(member)
            .map(|c| c.iter().collect())
            .unwrap_or_default()
    }

    /// Get who has this member as a contact
    pub fn get_contacted_by(&self, member: &str) -> Vec<&String> {
        self.contacted_by
            .get(member)
            .map(|c| c.iter().collect())
            .unwrap_or_default()
    }

    /// Check if there's a mutual contact relationship
    pub fn is_mutual(&self, member_a: &str, member_b: &str) -> bool {
        let a_has_b = self
            .contacts
            .get(member_a)
            .map(|c| c.contains(member_b))
            .unwrap_or(false);
        let b_has_a = self
            .contacts
            .get(member_b)
            .map(|c| c.contains(member_a))
            .unwrap_or(false);
        a_has_b && b_has_a
    }

    /// Get all edges (member -> contact) for graph visualization
    pub fn all_edges(&self) -> Vec<(&String, &String, bool)> {
        let mut edges = Vec::new();
        for (member, contacts) in &self.contacts {
            for contact in contacts {
                let mutual = self.is_mutual(member, contact);
                edges.push((member, contact, mutual));
            }
        }
        edges
    }

    /// Get member count
    pub fn member_count(&self) -> usize {
        self.all_members().len()
    }

    /// Get total contact relationships
    pub fn contact_count(&self) -> usize {
        self.contacts.values().map(|c| c.len()).sum()
    }

    /// Get all members relevant to a specific member (their contacts + who has them as contact)
    pub fn contacts_for_member(&self, member: &str) -> Vec<String> {
        let mut related: std::collections::HashSet<String> = std::collections::HashSet::new();

        // Add their contacts
        if let Some(contacts) = self.contacts.get(member) {
            for c in contacts {
                related.insert(c.clone());
            }
        }

        // Add who has them as contact
        if let Some(contacted_by) = self.contacted_by.get(member) {
            for c in contacted_by {
                related.insert(c.clone());
            }
        }

        let mut result: Vec<String> = related.into_iter().collect();
        result.sort();
        result
    }
}
