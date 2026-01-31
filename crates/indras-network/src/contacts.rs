//! Contacts - contact management and key exchange realm.
//!
//! The ContactsRealm is a special global realm used for managing contacts
//! and exchanging cryptographic keys. When you add someone as a contact,
//! you automatically subscribe to all peer-set realm combinations with them.

use crate::document::Document;
use crate::error::{IndraError, Result};
use crate::member::MemberId;
use crate::network::RealmId;

use indras_node::IndrasNode;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Arc;

/// Well-known identifier for the contacts realm.
/// This is deterministically derived from "indras:contacts:v1".
pub fn contacts_realm_id() -> RealmId {
    let hash = blake3::hash(b"indras:contacts:v1");
    indras_core::InterfaceId::new(*hash.as_bytes())
}

/// Metadata stored per contact.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContactEntry {
    /// Sentiment toward this contact: -1 = don't recommend, 0 = neutral, 1 = recommend.
    pub sentiment: i8,
    /// Whether this sentiment rating can be relayed to second-degree contacts.
    pub relayable: bool,
}

impl Default for ContactEntry {
    fn default() -> Self {
        Self {
            sentiment: 0,
            relayable: true,
        }
    }
}

/// Document schema for storing contacts with sentiment.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ContactsDocument {
    /// Contacts mapped to their entry metadata.
    pub contacts: BTreeMap<MemberId, ContactEntry>,
}

impl ContactsDocument {
    /// Create a new empty contacts document.
    pub fn new() -> Self {
        Self {
            contacts: BTreeMap::new(),
        }
    }

    /// Add a contact with default sentiment (neutral, relayable).
    pub fn add(&mut self, member_id: MemberId) {
        self.contacts
            .entry(member_id)
            .or_insert_with(ContactEntry::default);
    }

    /// Remove a contact.
    pub fn remove(&mut self, member_id: &MemberId) -> bool {
        self.contacts.remove(member_id).is_some()
    }

    /// Check if a member is a contact.
    pub fn contains(&self, member_id: &MemberId) -> bool {
        self.contacts.contains_key(member_id)
    }

    /// Get all contact IDs as a vector.
    pub fn list(&self) -> Vec<MemberId> {
        self.contacts.keys().copied().collect()
    }

    /// Get the number of contacts.
    pub fn len(&self) -> usize {
        self.contacts.len()
    }

    /// Check if contacts list is empty.
    pub fn is_empty(&self) -> bool {
        self.contacts.is_empty()
    }

    /// Set sentiment for a contact. Clamps to [-1, 1].
    pub fn set_sentiment(&mut self, member_id: &MemberId, sentiment: i8) -> bool {
        if let Some(entry) = self.contacts.get_mut(member_id) {
            entry.sentiment = sentiment.clamp(-1, 1);
            true
        } else {
            false
        }
    }

    /// Get sentiment for a contact.
    pub fn get_sentiment(&self, member_id: &MemberId) -> Option<i8> {
        self.contacts.get(member_id).map(|e| e.sentiment)
    }

    /// Set whether sentiment for a contact is relayable.
    pub fn set_relayable(&mut self, member_id: &MemberId, relayable: bool) -> bool {
        if let Some(entry) = self.contacts.get_mut(member_id) {
            entry.relayable = relayable;
            true
        } else {
            false
        }
    }

    /// Get the full entry for a contact.
    pub fn get_entry(&self, member_id: &MemberId) -> Option<&ContactEntry> {
        self.contacts.get(member_id)
    }

    /// Get all contacts with their sentiment values.
    pub fn contacts_with_sentiment(&self) -> Vec<(MemberId, i8)> {
        self.contacts
            .iter()
            .map(|(id, entry)| (*id, entry.sentiment))
            .collect()
    }

    /// Get only relayable sentiment entries (for second-degree relay).
    pub fn relayable_sentiments(&self) -> Vec<(MemberId, i8)> {
        self.contacts
            .iter()
            .filter(|(_, entry)| entry.relayable)
            .map(|(id, entry)| (*id, entry.sentiment))
            .collect()
    }
}

/// A wrapper around the contacts realm providing contact management.
///
/// The ContactsRealm is a special realm used for:
/// - Storing your contact list
/// - Exchanging cryptographic keys with contacts
/// - Auto-subscribing to peer-set realms with your contacts
///
/// # Example
///
/// ```ignore
/// // Join the contacts realm
/// let contacts = network.join_contacts_realm().await?;
///
/// // Add a friend
/// contacts.add_contact(friend_id).await?;
///
/// // List all contacts
/// for contact in contacts.contacts_list() {
///     println!("Contact: {:?}", contact);
/// }
/// ```
pub struct ContactsRealm {
    /// The realm ID (always contacts_realm_id()).
    id: RealmId,
    /// The contacts document.
    document: Document<ContactsDocument>,
    /// Reference to the underlying node.
    node: Arc<IndrasNode>,
    /// Our own member ID.
    self_id: MemberId,
}

impl ContactsRealm {
    /// Create a new ContactsRealm wrapper.
    pub(crate) async fn new(
        id: RealmId,
        node: Arc<IndrasNode>,
        self_id: MemberId,
    ) -> Result<Self> {
        let document = Document::new(id, "contacts".to_string(), Arc::clone(&node)).await?;

        Ok(Self {
            id,
            document,
            node,
            self_id,
        })
    }

    /// Get the realm ID.
    pub fn id(&self) -> RealmId {
        self.id
    }

    /// Get access to the contacts document.
    pub async fn contacts(&self) -> Result<Document<ContactsDocument>> {
        Ok(self.document.clone())
    }

    /// Add a contact.
    ///
    /// This adds the member to your contacts list and triggers
    /// auto-subscription to peer-set realms.
    pub async fn add_contact(&self, member_id: MemberId) -> Result<()> {
        // Don't add ourselves
        if member_id == self.self_id {
            return Err(IndraError::InvalidOperation(
                "Cannot add yourself as a contact".to_string(),
            ));
        }

        self.document
            .update(|doc| {
                doc.add(member_id);
            })
            .await?;

        Ok(())
    }

    /// Remove a contact.
    pub async fn remove_contact(&self, member_id: &MemberId) -> Result<bool> {
        let mut removed = false;
        self.document
            .update(|doc| {
                removed = doc.remove(member_id);
            })
            .await?;
        Ok(removed)
    }

    /// Check if a member is in your contacts.
    pub async fn is_contact(&self, member_id: &MemberId) -> bool {
        let doc = self.document.read().await;
        doc.contains(member_id)
    }

    /// Get the list of contacts.
    pub fn contacts_list(&self) -> Vec<MemberId> {
        self.document.read_blocking().list()
    }

    /// Get the number of contacts.
    pub fn contact_count(&self) -> usize {
        self.document.read_blocking().len()
    }

    /// Update sentiment for a contact. Clamps to [-1, 1].
    pub async fn update_sentiment(&self, member_id: &MemberId, sentiment: i8) -> Result<()> {
        let mid = *member_id;
        let mut updated = false;
        self.document
            .update(|doc| {
                updated = doc.set_sentiment(&mid, sentiment);
            })
            .await?;
        if !updated {
            return Err(IndraError::InvalidOperation(
                "Cannot set sentiment: member is not a contact".to_string(),
            ));
        }
        Ok(())
    }

    /// Get sentiment for a contact.
    pub fn get_sentiment(&self, member_id: &MemberId) -> Option<i8> {
        self.document.read_blocking().get_sentiment(member_id)
    }

    /// Get the full contact entry for a member.
    pub fn get_contact_entry(&self, member_id: &MemberId) -> Option<ContactEntry> {
        self.document.read_blocking().get_entry(member_id).cloned()
    }

    /// Set whether sentiment for a contact is relayable to second-degree contacts.
    pub async fn set_relayable(&self, member_id: &MemberId, relayable: bool) -> Result<()> {
        let mid = *member_id;
        let mut updated = false;
        self.document
            .update(|doc| {
                updated = doc.set_relayable(&mid, relayable);
            })
            .await?;
        if !updated {
            return Err(IndraError::InvalidOperation(
                "Cannot set relayable: member is not a contact".to_string(),
            ));
        }
        Ok(())
    }

    /// Get all contacts with their sentiment values.
    pub fn contacts_with_sentiment(&self) -> Vec<(MemberId, i8)> {
        self.document.read_blocking().contacts_with_sentiment()
    }

    /// Get relayable sentiments only (for publishing to second-degree contacts).
    pub fn relayable_sentiments(&self) -> Vec<(MemberId, i8)> {
        self.document.read_blocking().relayable_sentiments()
    }

    /// Access the underlying node.
    pub fn node(&self) -> &IndrasNode {
        &self.node
    }
}

impl Clone for ContactsRealm {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            document: self.document.clone(),
            node: Arc::clone(&self.node),
            self_id: self.self_id,
        }
    }
}

impl std::fmt::Debug for ContactsRealm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ContactsRealm")
            .field("id", &hex::encode(&self.id.as_bytes()[..8]))
            .field("contact_count", &self.contact_count())
            .finish()
    }
}

// Simple hex encoding for debug output
mod hex {
    pub fn encode(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{:02x}", b)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_contacts_document() {
        let mut doc = ContactsDocument::new();
        let member1 = [1u8; 32];
        let member2 = [2u8; 32];

        doc.add(member1);
        doc.add(member2);
        assert_eq!(doc.len(), 2);
        assert!(doc.contains(&member1));
        assert!(doc.contains(&member2));

        // Default sentiment is neutral
        assert_eq!(doc.get_sentiment(&member1), Some(0));
        assert_eq!(doc.get_sentiment(&member2), Some(0));

        doc.remove(&member1);
        assert_eq!(doc.len(), 1);
        assert!(!doc.contains(&member1));
        assert_eq!(doc.get_sentiment(&member1), None);
    }

    #[test]
    fn test_sentiment() {
        let mut doc = ContactsDocument::new();
        let member1 = [1u8; 32];
        let member2 = [2u8; 32];
        let noncontact = [3u8; 32];

        doc.add(member1);
        doc.add(member2);

        // Set sentiment
        assert!(doc.set_sentiment(&member1, 1));
        assert!(doc.set_sentiment(&member2, -1));
        assert!(!doc.set_sentiment(&noncontact, 1)); // not a contact

        assert_eq!(doc.get_sentiment(&member1), Some(1));
        assert_eq!(doc.get_sentiment(&member2), Some(-1));

        // Clamp to [-1, 1]
        doc.set_sentiment(&member1, 100);
        assert_eq!(doc.get_sentiment(&member1), Some(1));
        doc.set_sentiment(&member1, -100);
        assert_eq!(doc.get_sentiment(&member1), Some(-1));

        // Contacts with sentiment
        let with_sent = doc.contacts_with_sentiment();
        assert_eq!(with_sent.len(), 2);
    }

    #[test]
    fn test_relayable() {
        let mut doc = ContactsDocument::new();
        let member1 = [1u8; 32];

        doc.add(member1);

        // Default is relayable
        let entry = doc.get_entry(&member1).unwrap();
        assert!(entry.relayable);

        // Opt out of relay
        doc.set_relayable(&member1, false);
        let entry = doc.get_entry(&member1).unwrap();
        assert!(!entry.relayable);

        // Relayable sentiments should exclude opted-out contacts
        doc.set_sentiment(&member1, 1);
        assert!(doc.relayable_sentiments().is_empty());

        // Opt back in
        doc.set_relayable(&member1, true);
        assert_eq!(doc.relayable_sentiments().len(), 1);
    }

    #[test]
    fn test_contacts_realm_id() {
        // Should be deterministic
        let id1 = contacts_realm_id();
        let id2 = contacts_realm_id();
        assert_eq!(id1, id2);
    }
}
