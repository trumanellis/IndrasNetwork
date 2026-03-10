//! Action bridge wrapping HomeRealm + IndrasNetwork for gift cycle UI handlers.
//!
//! Thin wrapper that bundles the realm reference and local member identity
//! so UI handlers don't need to pass member_id explicitly. Provides methods
//! for all 6 stages of the gift cycle.

use std::sync::Arc;
use tokio::sync::RwLock;

use indras_network::error::{IndraError, Result};
use indras_network::home_realm::HomeRealm;
use indras_network::member::MemberId;
use indras_network::IndrasNetwork;
use indras_sync_engine::{
    AttentionDocument, AttentionEventId, BlessingDocument, BlessingId, ClaimId, Intention,
    IntentionDocument, IntentionId, IntentionKind, TokenOfGratitudeDocument, TokenOfGratitudeId,
    HomeRealmIntentions,
};

/// Handle wrapping a HomeRealm and local member identity for gift cycle actions.
///
/// `PartialEq` is implemented manually (compares member_id only) to satisfy
/// Dioxus component prop requirements.
#[derive(Clone)]
pub struct GiftCycleBridge {
    /// The home realm for CRDT operations.
    pub home: HomeRealm,
    /// The local member's identity.
    pub member_id: MemberId,
    /// Display name for the local member.
    pub player_name: String,
    /// The network instance for DM realm sharing.
    pub network: Arc<IndrasNetwork>,
    /// Shared homepage profile handle for live updates.
    pub homepage_profile: Option<Arc<RwLock<indras_profile::Profile>>>,
}

impl PartialEq for GiftCycleBridge {
    fn eq(&self, other: &Self) -> bool {
        self.member_id == other.member_id && self.player_name == other.player_name
    }
}

impl GiftCycleBridge {
    /// Create a new gift cycle bridge.
    pub fn new(
        home: HomeRealm,
        member_id: MemberId,
        player_name: String,
        network: Arc<IndrasNetwork>,
    ) -> Self {
        Self {
            home,
            member_id,
            player_name,
            network,
            homepage_profile: None,
        }
    }

    /// Set the homepage profile handle for live updates.
    pub fn with_homepage_profile(mut self, handle: Arc<RwLock<indras_profile::Profile>>) -> Self {
        self.homepage_profile = Some(handle);
        self
    }

    // ── Stage 1: Intention ─────────────────────────────────────────

    /// Create a new intention in the home realm.
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
        })
        .await?;
        Ok(intention_id)
    }

    /// Create an intention and share it to each audience peer's DM realm.
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

        for peer_id in &audience {
            match self.network.connect(*peer_id).await {
                Ok((dm_realm, _peer_info)) => {
                    let doc = dm_realm.document::<IntentionDocument>("intentions").await?;
                    let i = intention.clone();
                    doc.update(|d: &mut IntentionDocument| {
                        d.add(i);
                    })
                    .await?;

                    // Schedule delayed re-broadcasts so the peer catches the
                    // update even if they join the DM realm after the first send.
                    let net = self.network.clone();
                    let peer = *peer_id;
                    let i2 = intention.clone();
                    tokio::spawn(async move {
                        for delay_secs in [2, 5] {
                            tokio::time::sleep(std::time::Duration::from_secs(delay_secs)).await;
                            if let Ok((r, _)) = net.connect(peer).await {
                                if let Ok(doc) = r.document::<IntentionDocument>("intentions").await {
                                    let i = i2.clone();
                                    let _ = doc.update(|d: &mut IntentionDocument| {
                                        // add() is idempotent (keyed by intention.id)
                                        d.add(i);
                                    }).await;
                                }
                            }
                        }
                    });
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

        // Also write to home realm
        let home_doc = self.home.document::<IntentionDocument>("intentions").await?;
        let i = intention;
        home_doc
            .update(|d| {
                d.add(i);
            })
            .await?;

        Ok(intention_id)
    }

    // ── Stage 2: Attention ─────────────────────────────────────────

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

    /// Clear attention focus (idle).
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

    // ── Stage 3: Service ───────────────────────────────────────────

    /// Submit a service claim (proof of work) on an intention.
    pub async fn submit_proof(&self, intention_id: IntentionId) -> Result<usize> {
        self.home.submit_service_claim(intention_id, None).await
    }

    // ── Stage 4: Blessing ──────────────────────────────────────────

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

        // Validate blesser owns the attention events
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
                    "Event {} belongs to different member",
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

        // Mint a Token of Gratitude
        let token_doc = self
            .home
            .document::<TokenOfGratitudeDocument>("_tokens")
            .await?;
        let _token_id = token_doc
            .try_update(|d| {
                d.mint(claimant, blessing_id, blesser, intention_id, event_indices)
                    .map_err(|e| IndraError::InvalidOperation(e.to_string()))
            })
            .await?;

        Ok(blessing_id)
    }

    // ── Stage 5: Token ─────────────────────────────────────────────

    /// Pledge a token to an intention (staking gratitude as signal).
    pub async fn pledge_token(
        &self,
        token_id: TokenOfGratitudeId,
        intention_id: IntentionId,
    ) -> Result<()> {
        let token_doc = self
            .home
            .document::<TokenOfGratitudeDocument>("_tokens")
            .await?;
        {
            let guard = token_doc.read().await;
            match guard.find(&token_id) {
                Some(token) if token.steward != self.member_id => {
                    return Err(IndraError::InvalidOperation(
                        "Not authorized: only the steward can pledge".into(),
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
    #[allow(dead_code)]
    pub async fn release_token(
        &self,
        token_id: TokenOfGratitudeId,
        to: MemberId,
    ) -> Result<()> {
        let token_doc = self
            .home
            .document::<TokenOfGratitudeDocument>("_tokens")
            .await?;
        {
            let guard = token_doc.read().await;
            match guard.find(&token_id) {
                Some(token) if token.steward != self.member_id => {
                    return Err(IndraError::InvalidOperation(
                        "Not authorized: only the steward can release".into(),
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
        let token_doc = self
            .home
            .document::<TokenOfGratitudeDocument>("_tokens")
            .await?;
        {
            let guard = token_doc.read().await;
            match guard.find(&token_id) {
                Some(token) if token.steward != self.member_id => {
                    return Err(IndraError::InvalidOperation(
                        "Not authorized: only the steward can withdraw".into(),
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

    // ── Stage 6: Renewal ───────────────────────────────────────────
    // Renewal is: create a new intention with tagged tokens.
    // This reuses `create_intention` + `pledge_token`.

    /// Get connected peer IDs from conversation realms.
    pub fn connected_peers(&self) -> Vec<MemberId> {
        let mut peers = Vec::new();
        for realm_id in self.network.conversation_realms() {
            if let Some(peer_id) = self.network.dm_peer_for_realm(&realm_id) {
                if !peers.contains(&peer_id) {
                    peers.push(peer_id);
                }
            }
        }
        peers
    }

    /// Grant a peer access to Connections-level profile fields.
    pub async fn grant_profile_access(
        &self,
        grantee: MemberId,
        mode: indras_artifacts::AccessMode,
    ) -> Result<()> {
        let artifact_id = indras_artifacts::ArtifactId::Blob(
            indras_profile::profile_artifact_id(&self.member_id),
        );
        self.home.grant_access(&artifact_id, grantee, mode).await
    }
}
