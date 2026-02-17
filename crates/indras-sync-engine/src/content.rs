//! SyncContent — domain-specific content types for the SyncEngine.
//!
//! These are serialized into `Content::Extension` for transport over the
//! generic Indra's Network messaging layer.

use crate::proof_folder::ProofFolderId;
use crate::quest::QuestId;
use crate::token_of_gratitude::TokenOfGratitudeId;
use indras_network::member::MemberId;
use indras_network::message::{ContentReference, Content};
use serde::{Deserialize, Serialize};

/// The type identifier used in `Content::Extension` for SyncEngine content.
pub const SYNC_CONTENT_TYPE_ID: &str = "indras-sync-engine/v1";

/// Domain-specific content types for the SyncEngine app layer.
///
/// These variants were previously part of `Content` in indras-network.
/// They are now serialized into `Content::Extension { type_id, payload }`
/// for transport.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SyncContent {
    /// Proof submitted for a quest claim.
    ProofSubmitted {
        quest_id: QuestId,
        claimant: MemberId,
        artifact: ContentReference,
    },

    /// Blessing given to a quest proof.
    BlessingGiven {
        quest_id: QuestId,
        claimant: MemberId,
        blesser: MemberId,
        event_indices: Vec<usize>,
    },

    /// Proof folder submitted for review.
    ProofFolderSubmitted {
        quest_id: QuestId,
        claimant: MemberId,
        folder_id: ProofFolderId,
        narrative_preview: String,
        artifact_count: usize,
    },

    /// Gratitude pledged to a quest as a bounty.
    GratitudePledged {
        token_id: TokenOfGratitudeId,
        pledger: MemberId,
        target_quest_id: QuestId,
    },

    /// Gratitude released to a proof submitter.
    GratitudeReleased {
        token_id: TokenOfGratitudeId,
        from_steward: MemberId,
        to_steward: MemberId,
        target_quest_id: QuestId,
    },

    /// Gratitude pledge withdrawn by the steward.
    GratitudeWithdrawn {
        token_id: TokenOfGratitudeId,
        steward: MemberId,
        target_quest_id: QuestId,
    },
}

impl SyncContent {
    /// Serialize this SyncContent into a generic `Content::Extension`.
    pub fn to_content(&self) -> Content {
        let payload = postcard::to_allocvec(self).expect("SyncContent serialization");
        Content::Extension {
            type_id: SYNC_CONTENT_TYPE_ID.to_string(),
            payload,
        }
    }

    /// Try to parse a `Content::Extension` back into a `SyncContent`.
    ///
    /// Returns `None` if the content is not an Extension, or if the
    /// type_id doesn't match, or if deserialization fails.
    pub fn from_content(content: &Content) -> Option<Self> {
        match content {
            Content::Extension { type_id, payload } if type_id == SYNC_CONTENT_TYPE_ID => {
                postcard::from_bytes(payload).ok()
            }
            _ => None,
        }
    }

    /// Get the quest ID if this is a quest-related message.
    pub fn quest_id(&self) -> Option<&QuestId> {
        match self {
            SyncContent::ProofSubmitted { quest_id, .. } => Some(quest_id),
            SyncContent::BlessingGiven { quest_id, .. } => Some(quest_id),
            SyncContent::ProofFolderSubmitted { quest_id, .. } => Some(quest_id),
            SyncContent::GratitudePledged { target_quest_id, .. } => Some(target_quest_id),
            SyncContent::GratitudeReleased { target_quest_id, .. } => Some(target_quest_id),
            SyncContent::GratitudeWithdrawn { target_quest_id, .. } => Some(target_quest_id),
        }
    }

    /// Check if this is a proof submitted message.
    pub fn is_proof_submitted(&self) -> bool {
        matches!(self, SyncContent::ProofSubmitted { .. })
    }

    /// Check if this is a blessing given message.
    pub fn is_blessing_given(&self) -> bool {
        matches!(self, SyncContent::BlessingGiven { .. })
    }

    /// Check if this is a proof folder submitted message.
    pub fn is_proof_folder_submitted(&self) -> bool {
        matches!(self, SyncContent::ProofFolderSubmitted { .. })
    }

    /// Check if this is a gratitude pledged message.
    pub fn is_gratitude_pledged(&self) -> bool {
        matches!(self, SyncContent::GratitudePledged { .. })
    }

    /// Check if this is a gratitude released message.
    pub fn is_gratitude_released(&self) -> bool {
        matches!(self, SyncContent::GratitudeReleased { .. })
    }

    /// Check if this is a gratitude withdrawn message.
    pub fn is_gratitude_withdrawn(&self) -> bool {
        matches!(self, SyncContent::GratitudeWithdrawn { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sync_content_roundtrip() {
        let content = SyncContent::GratitudePledged {
            token_id: [1u8; 16],
            pledger: [2u8; 32],
            target_quest_id: [3u8; 16],
        };

        let generic = content.to_content();
        assert!(matches!(generic, Content::Extension { .. }));

        let recovered = SyncContent::from_content(&generic).unwrap();
        assert!(recovered.is_gratitude_pledged());
        assert_eq!(recovered.quest_id(), Some(&[3u8; 16]));
    }

    #[test]
    fn test_from_non_extension_returns_none() {
        let content = Content::Text("hello".to_string());
        assert!(SyncContent::from_content(&content).is_none());
    }

    #[test]
    fn test_from_wrong_type_id_returns_none() {
        let content = Content::Extension {
            type_id: "other-app/v1".to_string(),
            payload: vec![1, 2, 3],
        };
        assert!(SyncContent::from_content(&content).is_none());
    }
}
