//! Action bridge wrapping a HomeRealm for UI event handlers.
//!
//! Provides a thin wrapper around the home realm, bundling the
//! realm reference and local member identity so UI handlers
//! don't need to pass member_id explicitly.
//!
//! Intention CRUD uses the `HomeRealmIntentions` extension trait.
//! Attention, blessings, and tokens use direct CRDT document access
//! (the corresponding `Realm*` extension traits only exist for shared
//! Realm, not HomeRealm).

use indras_network::error::{IndraError, Result};
use indras_network::home_realm::HomeRealm;
use indras_network::member::MemberId;
use indras_sync_engine::{
    AttentionDocument, AttentionEventId, BlessingDocument, BlessingId,
    ClaimId, Intention, IntentionDocument, IntentionId, IntentionKind,
    TokenOfGratitudeDocument, TokenOfGratitudeId,
    HomeRealmIntentions,
};
use std::sync::Arc;
use indras_network::IndrasNetwork;

/// Handle wrapping a HomeRealm and local member identity for UI actions.
#[derive(Clone)]
pub struct RealmHandle {
    /// The home realm for CRDT operations.
    pub home: HomeRealm,
    /// The local member's identity.
    pub member_id: MemberId,
    /// Display name for the local member.
    pub player_name: String,
    /// The network instance for connecting to peer DM realms.
    pub network: Arc<IndrasNetwork>,
}

impl RealmHandle {
    /// Create a new realm handle.
    pub fn new(home: HomeRealm, member_id: MemberId, player_name: String, network: Arc<IndrasNetwork>) -> Self {
        Self { home, member_id, player_name, network }
    }

    // --- Intentions (via HomeRealmIntentions trait) ---

    /// Create a new intention in the home realm with a specific kind.
    pub async fn create_intention(
        &self,
        title: &str,
        description: &str,
        kind: IntentionKind,
    ) -> Result<IntentionId> {
        let mut intention = Intention::new(title, description, None, self.member_id);
        intention.kind = kind;
        let intention_id = intention.id;
        let doc = self.home.document::<IntentionDocument>("intentions").await?;
        doc.update(|d| {
            d.add(intention);
        }).await?;
        Ok(intention_id)
    }

    /// Create an intention and share it to each audience peer's DM realm.
    ///
    /// Writes the intention to each peer's DM realm (via `network.connect()`)
    /// and also to the creator's home realm so it appears in My Intentions.
    pub async fn create_dm_intention(
        &self,
        title: &str,
        description: &str,
        kind: IntentionKind,
        audience: Vec<MemberId>,
    ) -> Result<IntentionId> {
        let mut intention = Intention::new(title, description, None, self.member_id);
        intention.kind = kind;
        let intention_id = intention.id;

        // Write to each audience peer's DM realm
        for peer_id in &audience {
            match self.network.connect(*peer_id).await {
                Ok(dm_realm) => {
                    let doc = dm_realm.0.document::<IntentionDocument>("intentions").await?;
                    let i = intention.clone();
                    doc.update(|d| {
                        d.add(i);
                    }).await?;
                }
                Err(e) => {
                    tracing::warn!(
                        peer = %peer_id.iter().take(8).map(|b| format!("{b:02x}")).collect::<String>(),
                        error = %e,
                        "Failed to share intention to DM realm"
                    );
                }
            }
        }

        // Also write to home realm so creator sees it in My Intentions
        let home_doc = self.home.document::<IntentionDocument>("intentions").await?;
        let i = intention;
        home_doc.update(|d| {
            d.add(i);
        }).await?;

        Ok(intention_id)
    }

    /// Submit a service claim on an intention.
    pub async fn submit_proof(&self, intention_id: IntentionId) -> Result<usize> {
        self.home.submit_service_claim(intention_id, None).await
    }

    /// Verify a service claim (as the intention creator).
    pub async fn verify_claim(
        &self,
        intention_id: IntentionId,
        claim_index: usize,
    ) -> Result<()> {
        self.home.verify_service_claim(intention_id, claim_index).await
    }

    /// Mark an intention as completed.
    pub async fn complete_intention(&self, intention_id: IntentionId) -> Result<()> {
        self.home.complete_intention(intention_id).await
    }

    /// Update an intention's title and description.
    pub async fn update_intention(
        &self,
        intention_id: IntentionId,
        title: &str,
        description: &str,
    ) -> Result<()> {
        self.home.update_intention(intention_id, title, description).await
    }

    /// Delete an intention (tombstone).
    pub async fn delete_intention(&self, intention_id: IntentionId) -> Result<()> {
        self.home.delete_intention(intention_id).await
    }

    // --- Attention (direct document access) ---

    /// Focus attention on an intention.
    pub async fn focus_attention(
        &self,
        intention_id: IntentionId,
    ) -> Result<AttentionEventId> {
        let mut event_id = [0u8; 16];
        let doc = self.home.document::<AttentionDocument>("attention").await?;
        let member = self.member_id;
        doc.update(|d| {
            event_id = d.focus_on_intention(member, intention_id);
        })
        .await?;
        Ok(event_id)
    }

    /// Clear attention focus.
    pub async fn clear_attention(&self) -> Result<AttentionEventId> {
        let mut event_id = [0u8; 16];
        let doc = self.home.document::<AttentionDocument>("attention").await?;
        let member = self.member_id;
        doc.update(|d| {
            event_id = d.clear_attention(member);
        })
        .await?;
        Ok(event_id)
    }

    // --- Blessings (direct document access) ---

    /// Bless a service claim, minting a token of gratitude.
    pub async fn bless_claim(
        &self,
        intention_id: IntentionId,
        claimant: MemberId,
        event_indices: Vec<usize>,
    ) -> Result<BlessingId> {
        if event_indices.is_empty() {
            return Err(IndraError::InvalidOperation(
                "Cannot bless with empty event indices".into(),
            ));
        }

        let blesser = self.member_id;

        // Validate that blesser owns the attention events
        let attention_doc = self.home.document::<AttentionDocument>("attention").await?;
        let attention = attention_doc.read().await;
        let events = attention.events();
        for &idx in &event_indices {
            if idx >= events.len() {
                return Err(IndraError::InvalidOperation(format!(
                    "Invalid event index: {} (only {} events exist)",
                    idx,
                    events.len()
                )));
            }
            let event = &events[idx];
            if event.member != blesser {
                return Err(IndraError::InvalidOperation(format!(
                    "Event {} belongs to different member, not blesser",
                    idx
                )));
            }
            if event.intention_id != Some(intention_id) {
                return Err(IndraError::InvalidOperation(format!(
                    "Event {} is for different intention",
                    idx
                )));
            }
        }
        drop(attention);

        // Record the blessing
        let claim_id = ClaimId::new(intention_id, claimant);
        let blessing_doc = self.home.document::<BlessingDocument>("blessings").await?;
        let event_indices_clone = event_indices.clone();
        let blessing_id = blessing_doc
            .try_update(|d| {
                d.bless_claim(claim_id, blesser, event_indices_clone)
                    .map_err(|e| IndraError::InvalidOperation(e.to_string()))
            })
            .await?;

        // Mint a Token of Gratitude for the claimant
        let token_doc = self.home.document::<TokenOfGratitudeDocument>("_tokens").await?;
        let _token_id = token_doc
            .try_update(|d| {
                d.mint(claimant, blessing_id, blesser, intention_id, event_indices)
                    .map_err(|e| IndraError::InvalidOperation(e.to_string()))
            })
            .await?;

        Ok(blessing_id)
    }

    // --- Tokens (direct document access) ---

    /// Pledge a token to an intention.
    pub async fn pledge_token(
        &self,
        token_id: TokenOfGratitudeId,
        intention_id: IntentionId,
    ) -> Result<()> {
        let token_doc = self.home.document::<TokenOfGratitudeDocument>("_tokens").await?;
        {
            let guard = token_doc.read().await;
            match guard.find(&token_id) {
                Some(token) if token.steward != self.member_id => {
                    return Err(IndraError::InvalidOperation(
                        "Not authorized: only the token's steward can pledge it".into(),
                    ));
                }
                None => {
                    return Err(IndraError::InvalidOperation("Token not found".into()));
                }
                _ => {}
            }
        }
        token_doc
            .try_update(|d| {
                d.pledge(token_id, intention_id)
                    .map_err(|e| IndraError::InvalidOperation(e.to_string()))
            })
            .await?;
        Ok(())
    }

    /// Release a token to a new steward.
    pub async fn release_token(
        &self,
        token_id: TokenOfGratitudeId,
        to: MemberId,
    ) -> Result<()> {
        let token_doc = self.home.document::<TokenOfGratitudeDocument>("_tokens").await?;
        {
            let guard = token_doc.read().await;
            match guard.find(&token_id) {
                Some(token) if token.steward != self.member_id => {
                    return Err(IndraError::InvalidOperation(
                        "Not authorized: only the token's steward can release it".into(),
                    ));
                }
                None => {
                    return Err(IndraError::InvalidOperation("Token not found".into()));
                }
                _ => {}
            }
        }
        token_doc
            .try_update(|d| {
                d.release(token_id, to)
                    .map_err(|e| IndraError::InvalidOperation(e.to_string()))
            })
            .await?;
        Ok(())
    }

    /// Withdraw a pledged token.
    pub async fn withdraw_token(&self, token_id: TokenOfGratitudeId) -> Result<()> {
        let token_doc = self.home.document::<TokenOfGratitudeDocument>("_tokens").await?;
        {
            let guard = token_doc.read().await;
            match guard.find(&token_id) {
                Some(token) if token.steward != self.member_id => {
                    return Err(IndraError::InvalidOperation(
                        "Not authorized: only the token's steward can withdraw it".into(),
                    ));
                }
                None => {
                    return Err(IndraError::InvalidOperation("Token not found".into()));
                }
                _ => {}
            }
        }
        token_doc
            .try_update(|d| {
                d.withdraw(token_id)
                    .map_err(|e| IndraError::InvalidOperation(e.to_string()))
            })
            .await?;
        Ok(())
    }
}
