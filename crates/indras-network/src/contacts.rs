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
use std::collections::BTreeSet;
use std::sync::Arc;

/// Well-known identifier for the contacts realm.
/// This is deterministically derived from "indras:contacts:v1".
pub fn contacts_realm_id() -> RealmId {
    let hash = blake3::hash(b"indras:contacts:v1");
    indras_core::InterfaceId::new(*hash.as_bytes())
}

/// Document schema for storing contacts.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ContactsDocument {
    /// Set of contact member IDs.
    pub contacts: BTreeSet<MemberId>,
}

impl ContactsDocument {
    /// Create a new empty contacts document.
    pub fn new() -> Self {
        Self {
            contacts: BTreeSet::new(),
        }
    }

    /// Add a contact.
    pub fn add(&mut self, member_id: MemberId) {
        self.contacts.insert(member_id);
    }

    /// Remove a contact.
    pub fn remove(&mut self, member_id: &MemberId) -> bool {
        self.contacts.remove(member_id)
    }

    /// Check if a member is a contact.
    pub fn contains(&self, member_id: &MemberId) -> bool {
        self.contacts.contains(member_id)
    }

    /// Get all contacts as a vector.
    pub fn list(&self) -> Vec<MemberId> {
        self.contacts.iter().copied().collect()
    }

    /// Get the number of contacts.
    pub fn len(&self) -> usize {
        self.contacts.len()
    }

    /// Check if contacts list is empty.
    pub fn is_empty(&self) -> bool {
        self.contacts.is_empty()
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

        doc.remove(&member1);
        assert_eq!(doc.len(), 1);
        assert!(!doc.contains(&member1));
    }

    #[test]
    fn test_contacts_realm_id() {
        // Should be deterministic
        let id1 = contacts_realm_id();
        let id2 = contacts_realm_id();
        assert_eq!(id1, id2);
    }
}
