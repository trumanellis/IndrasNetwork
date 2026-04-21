//! Steward enrollment handshake — invitation and acceptance CRDT docs.
//!
//! When a user nominates a DM peer as a steward, they publish a
//! [`StewardInvitation`] into the sender↔peer DM realm under the key
//! [`invite_doc_key`]. The peer's device reads the invitation out of
//! an inbox view and, after human approval, replies with a
//! [`StewardResponse`] under [`response_doc_key`]. When `N`
//! acceptances land the sender's device splits the secret and
//! publishes encrypted shares (see
//! [`crate::share_delivery`]).
//!
//! One invitation per sender — re-inviting bumps the timestamp so
//! merge picks the latest state. Revoking is modeled as an
//! invitation with an empty `responsibility_text` and the
//! `withdrawn` flag set; responses to a withdrawn invitation are
//! ignored on the sender side.
//!
//! This module intentionally carries only the *metadata* of the
//! handshake. The actual encrypted share (published after quorum)
//! continues to use the existing `_steward_share:*` doc.

use serde::{Deserialize, Serialize};

use indras_network::document::DocumentSchema;

/// Doc key prefix for the invitation doc.
pub const INVITE_KEY_PREFIX: &str = "_steward_invite:";

/// Doc key prefix for the response doc.
pub const RESPONSE_KEY_PREFIX: &str = "_steward_accept:";

/// The doc key a user writes to invite a specific DM peer to be a
/// steward. The hex is the *sender's* `UserId`, so a steward reads
/// this under each DM peer's UID and a user only ever writes their
/// own.
pub fn invite_doc_key(sender_user_id: &[u8; 32]) -> String {
    format!("{}{}", INVITE_KEY_PREFIX, hex::encode(sender_user_id))
}

/// The doc key a steward writes back to acknowledge or decline an
/// invitation. Same sender-UID convention — the steward writes into
/// the DM realm they share with the sender, keyed by the sender's
/// UID, so the sender's device reads per-peer responses by scanning
/// their own UID across all DM realms.
pub fn response_doc_key(sender_user_id: &[u8; 32]) -> String {
    format!("{}{}", RESPONSE_KEY_PREFIX, hex::encode(sender_user_id))
}

/// A user's request for a DM peer to act as a steward.
///
/// The body carries a plain-language responsibility description
/// rendered verbatim in the steward's approval dialog — no crypto
/// vocabulary surfaces in the UI.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StewardInvitation {
    /// Sender's `UserId`. Readers verify this matches the doc key
    /// suffix so a malicious writer can't spoof another peer.
    pub from_user_id: [u8; 32],
    /// Sender's display name at invitation time. Rendered in the
    /// steward's approval dialog.
    pub from_display_name: String,
    /// Plain-language description of what being a steward means.
    /// Defaults to the copy shipped in this module but senders can
    /// personalize.
    pub responsibility_text: String,
    /// Number of stewards that must release a share to complete a
    /// recovery (K in K-of-N).
    pub threshold_k: u8,
    /// Total stewards the sender is aiming to enroll (N in K-of-N).
    pub total_n: u8,
    /// Wall-clock millis when the invitation was issued.
    pub issued_at_millis: i64,
    /// `true` when the sender has withdrawn the invitation. A
    /// withdrawn invitation supersedes any prior acceptance.
    pub withdrawn: bool,
}

impl DocumentSchema for StewardInvitation {
    fn merge(&mut self, remote: Self) {
        if remote.issued_at_millis > self.issued_at_millis {
            *self = remote;
        }
    }
}

/// Default copy used when a sender doesn't supply their own
/// responsibility text. Kept plain-language per DESIGN.md.
pub const DEFAULT_RESPONSIBILITY: &str =
    "If they ever lose their device, they'll ask you to help them get back in. \
Before you approve, make sure it's really them — call them, video-chat them, \
see them in person. You just tap Approve once you're sure.";

/// The steward's response to an invitation.
///
/// A fresh ML-KEM encapsulation key is included so the sender can
/// wrap the per-steward share directly to it, which lets the steward
/// rotate their key without re-enrolling.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StewardResponse {
    /// The responding steward's `UserId`.
    pub steward_user_id: [u8; 32],
    /// Whether the steward has accepted the invitation.
    pub accepted: bool,
    /// Wall-clock millis when the response was written.
    pub responded_at_millis: i64,
    /// Steward's fresh ML-KEM-768 encapsulation key. Empty when
    /// `accepted == false`.
    pub kem_ek_bytes: Vec<u8>,
    /// Steward's DSA verifying key, echoed back so the sender can
    /// tie the acceptance to a known identity.
    pub dsa_vk_bytes: Vec<u8>,
}

impl DocumentSchema for StewardResponse {
    fn merge(&mut self, remote: Self) {
        if remote.responded_at_millis > self.responded_at_millis {
            *self = remote;
        }
    }
}

/// One sender-side view of a single peer's enrollment state.
///
/// Produced by scanning the outgoing invitation and the peer's
/// response. Used to drive the Backup-plan overlay's per-peer status
/// badge — no crypto vocabulary leaks through this struct.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnrollmentStatus {
    /// No invitation issued yet.
    NotInvited,
    /// Invitation issued, no response received.
    Invited {
        /// Wall-clock millis of invitation.
        issued_at_millis: i64,
    },
    /// Peer accepted.
    Accepted {
        /// Wall-clock millis of acceptance.
        responded_at_millis: i64,
    },
    /// Peer declined.
    Declined {
        /// Wall-clock millis of decline.
        responded_at_millis: i64,
    },
    /// Invitation was withdrawn by the sender.
    Withdrawn,
}

impl EnrollmentStatus {
    /// Derive the status from a pair of (invitation, response). Both
    /// may be `None` for a peer we've never interacted with. Keeps
    /// this module self-contained and testable without pulling in
    /// the network layer.
    pub fn derive(
        invite: Option<&StewardInvitation>,
        response: Option<&StewardResponse>,
    ) -> Self {
        match (invite, response) {
            (None, _) => Self::NotInvited,
            (Some(inv), _) if inv.withdrawn => Self::Withdrawn,
            (Some(inv), None) => Self::Invited {
                issued_at_millis: inv.issued_at_millis,
            },
            (Some(inv), Some(resp))
                if resp.responded_at_millis < inv.issued_at_millis =>
            {
                // Stale response predates the latest invitation — treat
                // as if the invitation is still pending.
                Self::Invited {
                    issued_at_millis: inv.issued_at_millis,
                }
            }
            (Some(_), Some(resp)) if resp.accepted => Self::Accepted {
                responded_at_millis: resp.responded_at_millis,
            },
            (Some(_), Some(resp)) => Self::Declined {
                responded_at_millis: resp.responded_at_millis,
            },
        }
    }

    /// Quick check used by the Backup-plan overlay's quorum counter.
    pub fn is_accepted(&self) -> bool {
        matches!(self, Self::Accepted { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doc_keys_are_stable_and_distinct() {
        let uid = [0xabu8; 32];
        let i1 = invite_doc_key(&uid);
        let i2 = invite_doc_key(&uid);
        let r1 = response_doc_key(&uid);

        assert_eq!(i1, i2);
        assert_ne!(i1, r1);
        assert!(i1.starts_with(INVITE_KEY_PREFIX));
        assert!(r1.starts_with(RESPONSE_KEY_PREFIX));
        assert_eq!(i1.len(), INVITE_KEY_PREFIX.len() + 64);
        assert_eq!(r1.len(), RESPONSE_KEY_PREFIX.len() + 64);
    }

    #[test]
    fn invitation_merge_prefers_newer() {
        let older = StewardInvitation {
            from_user_id: [1u8; 32],
            from_display_name: "Alex".into(),
            responsibility_text: "v1".into(),
            threshold_k: 3,
            total_n: 5,
            issued_at_millis: 1_000,
            withdrawn: false,
        };
        let newer = StewardInvitation {
            responsibility_text: "v2".into(),
            issued_at_millis: 5_000,
            ..older.clone()
        };

        let mut a = older.clone();
        a.merge(newer.clone());
        assert_eq!(a.issued_at_millis, 5_000);
        assert_eq!(a.responsibility_text, "v2");

        let mut b = newer.clone();
        b.merge(older);
        assert_eq!(b.issued_at_millis, 5_000);
        assert_eq!(b.responsibility_text, "v2");
    }

    #[test]
    fn response_merge_prefers_newer() {
        let decline = StewardResponse {
            steward_user_id: [2u8; 32],
            accepted: false,
            responded_at_millis: 2_000,
            kem_ek_bytes: vec![],
            dsa_vk_bytes: vec![0xcc; 8],
        };
        let accept = StewardResponse {
            accepted: true,
            responded_at_millis: 4_000,
            kem_ek_bytes: vec![0x11, 0x22],
            ..decline.clone()
        };

        let mut a = decline.clone();
        a.merge(accept.clone());
        assert!(a.accepted);
        assert_eq!(a.responded_at_millis, 4_000);

        let mut b = accept.clone();
        b.merge(decline);
        assert!(b.accepted);
    }

    #[test]
    fn derive_status_covers_all_cases() {
        let now = 10_000;
        let inv = StewardInvitation {
            from_user_id: [3u8; 32],
            from_display_name: "Sam".into(),
            responsibility_text: "…".into(),
            threshold_k: 2,
            total_n: 3,
            issued_at_millis: now,
            withdrawn: false,
        };
        let accept = StewardResponse {
            steward_user_id: [4u8; 32],
            accepted: true,
            responded_at_millis: now + 1_000,
            kem_ek_bytes: vec![9],
            dsa_vk_bytes: vec![8],
        };
        let decline = StewardResponse {
            accepted: false,
            responded_at_millis: now + 2_000,
            ..accept.clone()
        };
        let stale = StewardResponse {
            accepted: true,
            responded_at_millis: now - 100, // older than invitation
            ..accept.clone()
        };
        let withdrawn = StewardInvitation {
            withdrawn: true,
            ..inv.clone()
        };

        assert_eq!(EnrollmentStatus::derive(None, None), EnrollmentStatus::NotInvited);
        assert!(matches!(
            EnrollmentStatus::derive(Some(&inv), None),
            EnrollmentStatus::Invited { .. }
        ));
        assert!(matches!(
            EnrollmentStatus::derive(Some(&inv), Some(&accept)),
            EnrollmentStatus::Accepted { .. }
        ));
        assert!(matches!(
            EnrollmentStatus::derive(Some(&inv), Some(&decline)),
            EnrollmentStatus::Declined { .. }
        ));
        assert!(matches!(
            EnrollmentStatus::derive(Some(&inv), Some(&stale)),
            EnrollmentStatus::Invited { .. }
        ));
        assert_eq!(
            EnrollmentStatus::derive(Some(&withdrawn), Some(&accept)),
            EnrollmentStatus::Withdrawn
        );
    }

    #[test]
    fn is_accepted_only_matches_accepted_variant() {
        assert!(EnrollmentStatus::Accepted {
            responded_at_millis: 0
        }
        .is_accepted());
        assert!(!EnrollmentStatus::NotInvited.is_accepted());
        assert!(!EnrollmentStatus::Invited {
            issued_at_millis: 0
        }
        .is_accepted());
        assert!(!EnrollmentStatus::Declined {
            responded_at_millis: 0
        }
        .is_accepted());
        assert!(!EnrollmentStatus::Withdrawn.is_accepted());
    }
}
