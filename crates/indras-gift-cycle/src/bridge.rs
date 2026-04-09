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
use indras_network::{IndrasNetwork, RealmId};
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
    /// Shared homepage fields handle for live updates.
    pub homepage_fields: Option<Arc<RwLock<Vec<indras_homepage::ProfileFieldArtifact>>>>,
    /// Shared homepage artifacts handle for live updates.
    pub homepage_artifacts: Option<Arc<RwLock<Vec<indras_homepage::ContentArtifact>>>>,
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
            homepage_fields: None,
            homepage_artifacts: None,
        }
    }

    /// Set the homepage fields handle for live updates.
    pub fn with_homepage_fields(mut self, handle: Arc<RwLock<Vec<indras_homepage::ProfileFieldArtifact>>>) -> Self {
        self.homepage_fields = Some(handle);
        self
    }

    /// Set the homepage artifacts handle for live updates.
    pub fn with_homepage_artifacts(mut self, handle: Arc<RwLock<Vec<indras_homepage::ContentArtifact>>>) -> Self {
        self.homepage_artifacts = Some(handle);
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
    ///
    /// Creates the event ONCE, then inserts the exact same event (same `event_id`,
    /// same timestamp) into all relevant docs. For home realm intentions, the event
    /// is written to home + all DM realms. For DM realm intentions, the event is
    /// written to the source DM realm + home realm. CRDT dedup by `event_id`
    /// prevents double-counting.
    pub async fn focus_attention(
        &self,
        intention_id: IntentionId,
        source_realm_id: Option<RealmId>,
    ) -> Result<AttentionEventId> {
        use indras_sync_engine::AttentionSwitchEvent;

        let event = AttentionSwitchEvent::focus(self.member_id, intention_id);
        let event_id = event.event_id;

        if let Some(ref rid) = source_realm_id {
            // Community/DM intention — write to the source DM realm AND home realm
            // so the home realm has a complete record for card view.
            let realm = self.network.get_realm_by_id(rid)
                .ok_or_else(|| IndraError::RealmNotFound { id: "source realm".into() })?;
            let doc = realm.document::<AttentionDocument>("attention").await?;
            let ev = event.clone();
            doc.update(|d| { d.insert_event(ev); }).await?;

            // Mirror to home realm for complete attention history
            let home_doc = self.home.document::<AttentionDocument>("attention").await?;
            let ev = event.clone();
            home_doc.update(|d| { d.insert_event(ev); }).await?;
        } else {
            // Home realm intention — write to home + all DM realms
            let home_doc = self.home.document::<AttentionDocument>("attention").await?;
            let ev = event.clone();
            home_doc.update(|d| { d.insert_event(ev); }).await?;

            for rid in self.network.conversation_realms() {
                let Some(peer_id) = self.network.dm_peer_for_realm(&rid) else { continue };
                let Some(realm) = self.network.get_realm_by_id(&rid) else { continue };
                let Ok(doc) = realm.document::<AttentionDocument>("attention").await else { continue };
                let ev = event.clone();
                let _ = doc.update(|d| { d.insert_event(ev); }).await;

                // Retry inserts so late-joining peers don't miss the event
                let net = self.network.clone();
                let ev_clone = event.clone();
                tokio::spawn(async move {
                    for delay_secs in [2u64, 5] {
                        tokio::time::sleep(std::time::Duration::from_secs(delay_secs)).await;
                        if let Ok((r, _)) = net.connect(peer_id).await {
                            if let Ok(doc) = r.document::<AttentionDocument>("attention").await {
                                // Check before writing to avoid unnecessary sync traffic
                                let already_present = {
                                    let data = doc.read().await;
                                    data.events().iter().any(|e| e.event_id == ev_clone.event_id)
                                };
                                if !already_present {
                                    let ev = ev_clone.clone();
                                    let _ = doc.update(|d: &mut AttentionDocument| {
                                        d.insert_event(ev);
                                    }).await;
                                }
                            }
                        }
                    }
                });
            }
        }
        Ok(event_id)
    }

    /// Clear attention focus (idle).
    ///
    /// Creates the clear event ONCE, then inserts the exact same event into all
    /// relevant docs. Same pattern as `focus_attention`.
    pub async fn clear_attention(&self, source_realm_id: Option<RealmId>) -> Result<AttentionEventId> {
        use indras_sync_engine::AttentionSwitchEvent;

        let event = AttentionSwitchEvent::clear(self.member_id);
        let event_id = event.event_id;

        if let Some(ref rid) = source_realm_id {
            // Community/DM intention — write to the source DM realm AND home realm
            let realm = self.network.get_realm_by_id(rid)
                .ok_or_else(|| IndraError::RealmNotFound { id: "source realm".into() })?;
            let doc = realm.document::<AttentionDocument>("attention").await?;
            let ev = event.clone();
            doc.update(|d| { d.insert_event(ev); }).await?;

            // Mirror to home realm for complete attention history
            let home_doc = self.home.document::<AttentionDocument>("attention").await?;
            let ev = event.clone();
            home_doc.update(|d| { d.insert_event(ev); }).await?;
        } else {
            let home_doc = self.home.document::<AttentionDocument>("attention").await?;
            let ev = event.clone();
            home_doc.update(|d| { d.insert_event(ev); }).await?;

            for rid in self.network.conversation_realms() {
                let Some(peer_id) = self.network.dm_peer_for_realm(&rid) else { continue };
                let Some(realm) = self.network.get_realm_by_id(&rid) else { continue };
                let Ok(doc) = realm.document::<AttentionDocument>("attention").await else { continue };
                let ev = event.clone();
                let _ = doc.update(|d| { d.insert_event(ev); }).await;

                // Retry inserts so late-joining peers don't miss the event
                let net = self.network.clone();
                let ev_clone = event.clone();
                tokio::spawn(async move {
                    for delay_secs in [2u64, 5] {
                        tokio::time::sleep(std::time::Duration::from_secs(delay_secs)).await;
                        if let Ok((r, _)) = net.connect(peer_id).await {
                            if let Ok(doc) = r.document::<AttentionDocument>("attention").await {
                                // Check before writing to avoid unnecessary sync traffic
                                let already_present = {
                                    let data = doc.read().await;
                                    data.events().iter().any(|e| e.event_id == ev_clone.event_id)
                                };
                                if !already_present {
                                    let ev = ev_clone.clone();
                                    let _ = doc.update(|d: &mut AttentionDocument| {
                                        d.insert_event(ev);
                                    }).await;
                                }
                            }
                        }
                    }
                });
            }
        }
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

    /// Set a profile field to public visibility.
    ///
    /// Replaces all grants with a single public grant.
    pub async fn set_field_public(&self, field_name: &str) -> Result<()> {
        let artifact_id = indras_artifacts::ArtifactId::Doc(
            indras_homepage::profile_field_artifact_id(&self.member_id, field_name),
        );
        let doc = self.home.artifact_index().await?;
        let member_id = self.member_id;
        let grants = vec![indras_artifacts::AccessGrant {
            grantee: [0u8; 32],
            mode: indras_artifacts::AccessMode::Public,
            granted_at: 0,
            granted_by: member_id,
        }];
        doc.update(|index| {
            index.replace_grants(&artifact_id, grants.clone());
        })
        .await?;
        Ok(())
    }

    /// Set a profile field to connections-only visibility.
    ///
    /// Replaces all grants with Revocable grants for each current contact.
    pub async fn set_field_connections_only(&self, field_name: &str) -> Result<()> {
        let artifact_id = indras_artifacts::ArtifactId::Doc(
            indras_homepage::profile_field_artifact_id(&self.member_id, field_name),
        );
        let doc = self.home.artifact_index().await?;
        let member_id = self.member_id;

        // Get current contacts
        let contacts: Vec<[u8; 32]> = if let Some(contacts_realm) = self.network.contacts_realm().await {
            if let Ok(cdoc) = contacts_realm.contacts().await {
                let data = cdoc.read().await;
                data.contacts.keys().copied().collect()
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        let grants: Vec<indras_artifacts::AccessGrant> = contacts
            .iter()
            .filter(|c| **c != member_id && **c != [0u8; 32])
            .map(|contact| indras_artifacts::AccessGrant {
                grantee: *contact,
                mode: indras_artifacts::AccessMode::Revocable,
                granted_at: 0,
                granted_by: member_id,
            })
            .collect();

        doc.update(move |index| {
            index.replace_grants(&artifact_id, grants.clone());
        })
        .await?;
        Ok(())
    }

    /// Set a profile field to private (remove all grants).
    pub async fn set_field_private(&self, field_name: &str) -> Result<()> {
        let artifact_id = indras_artifacts::ArtifactId::Doc(
            indras_homepage::profile_field_artifact_id(&self.member_id, field_name),
        );
        let doc = self.home.artifact_index().await?;
        doc.update(|index| {
            index.replace_grants(&artifact_id, Vec::new());
        })
        .await?;
        Ok(())
    }

    /// Grant timed access to a profile field for a specific peer.
    pub async fn grant_field_timed_access(
        &self,
        field_name: &str,
        grantee: MemberId,
        duration_secs: i64,
    ) -> Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        let expires_at = now + duration_secs;
        self.grant_field_access(
            field_name,
            grantee,
            indras_artifacts::AccessMode::Timed { expires_at },
        )
        .await
    }

    /// Revoke a specific peer's access to a profile field.
    pub async fn revoke_field_access(&self, field_name: &str, grantee: MemberId) -> Result<()> {
        let artifact_id = indras_artifacts::ArtifactId::Doc(
            indras_homepage::profile_field_artifact_id(&self.member_id, field_name),
        );
        let doc = self.home.artifact_index().await?;
        doc.update(|index| {
            if let Some(entry) = index.get(&artifact_id) {
                let grants: Vec<_> = entry
                    .grants
                    .iter()
                    .filter(|g| g.grantee != grantee)
                    .cloned()
                    .collect();
                index.replace_grants(&artifact_id, grants);
            }
        })
        .await?;
        Ok(())
    }

    /// Sync connections-only grants for all fields that have that visibility.
    ///
    /// For each field with non-public, non-empty grants, ensures grants match
    /// the current contact list (adds new contacts, revokes removed ones).
    pub async fn sync_connections_only_grants(&self) -> Result<()> {
        let contacts: Vec<[u8; 32]> = if let Some(contacts_realm) = self.network.contacts_realm().await {
            if let Ok(cdoc) = contacts_realm.contacts().await {
                let data = cdoc.read().await;
                data.contacts.keys().copied().collect()
            } else {
                return Ok(());
            }
        } else {
            return Ok(());
        };

        let doc = self.home.artifact_index().await?;
        let member_id = self.member_id;
        let field_names: Vec<&str> = vec![
            indras_homepage::fields::DISPLAY_NAME,
            indras_homepage::fields::USERNAME,
            indras_homepage::fields::BIO,
            indras_homepage::fields::PUBLIC_KEY,
            indras_homepage::fields::INTENTION_COUNT,
            indras_homepage::fields::TOKEN_COUNT,
            indras_homepage::fields::BLESSINGS_GIVEN,
            indras_homepage::fields::ATTENTION_CONTRIBUTED,
            indras_homepage::fields::CONTACT_COUNT,
            indras_homepage::fields::HUMANNESS_FRESHNESS,
            indras_homepage::fields::ACTIVE_QUESTS,
            indras_homepage::fields::ACTIVE_OFFERINGS,
        ];

        doc.update(move |index| {
            let contact_set: std::collections::HashSet<[u8; 32]> = contacts
                .iter()
                .filter(|c| **c != member_id && **c != [0u8; 32])
                .copied()
                .collect();

            for field_name in &field_names {
                let aid = indras_artifacts::ArtifactId::Doc(
                    indras_homepage::profile_field_artifact_id(&member_id, field_name),
                );
                let is_connections_only = if let Some(entry) = index.get(&aid) {
                    let has_public = entry
                        .grants
                        .iter()
                        .any(|g| matches!(g.mode, indras_artifacts::AccessMode::Public));
                    !has_public && !entry.grants.is_empty()
                } else {
                    false
                };

                if !is_connections_only {
                    continue; // Skip public, private, and missing fields
                }

                // Build new grants from current contact list
                let grants: Vec<indras_artifacts::AccessGrant> = contact_set
                    .iter()
                    .map(|contact| indras_artifacts::AccessGrant {
                        grantee: *contact,
                        mode: indras_artifacts::AccessMode::Revocable,
                        granted_at: 0,
                        granted_by: member_id,
                    })
                    .collect();

                index.replace_grants(&aid, grants);
            }
        })
        .await?;
        Ok(())
    }

    /// Grant a peer access to a specific profile field.
    pub async fn grant_field_access(
        &self,
        field_name: &str,
        grantee: MemberId,
        mode: indras_artifacts::AccessMode,
    ) -> Result<()> {
        let artifact_id = indras_artifacts::ArtifactId::Doc(
            indras_homepage::profile_field_artifact_id(&self.member_id, field_name),
        );
        self.home.grant_access(&artifact_id, grantee, mode).await
    }
}
