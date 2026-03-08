//! Extension trait adding attention tracking methods to Realm.

use crate::attention::{AttentionDocument, AttentionEventId, IntentionAttention};
use crate::attention_tip::{AttentionTip, AttentionTipDocument};
use crate::certificate::CertificateDocument;
use crate::fraud_evidence::{FraudEvidenceDocument, FraudRecord};
use crate::humanness::HumannessDocument;
use crate::intention::IntentionId;
use crate::witness_roster::WitnessRosterDocument;
use indras_artifacts::attention::certificate::{QuorumCertificate, WitnessSignature};
use indras_artifacts::attention::AttentionSwitchEvent as ChainedSwitchEvent;
use indras_artifacts::attention::validate::AuthorState;
use indras_artifacts::artifact::ArtifactId;
use indras_crypto::{PQIdentity, PQPublicIdentity};
use indras_network::document::Document;
use indras_network::error::{IndraError, Result};
use indras_network::member::MemberId;
use indras_network::Realm;
use std::collections::HashMap;

/// Intention attention weighted by humanness freshness.
///
/// Each member's attention contribution is multiplied by their humanness
/// freshness (0.0–1.0). Fresh attestation = full weight, stale = reduced,
/// absent = zero. This makes Sybil accounts' attention invisible.
#[derive(Debug, Clone)]
pub struct WeightedQuestAttention {
    /// The intention.
    pub intention_id: IntentionId,
    /// Raw total attention (unweighted, same as `QuestAttention`).
    pub raw_attention_millis: u64,
    /// Weighted total: sum of each member's (millis × freshness).
    pub weighted_attention_millis: f64,
    /// Per-member breakdown: (raw_millis, freshness, weighted_millis).
    pub by_member: HashMap<MemberId, (u64, f64, f64)>,
}

/// Attention tracking extension trait for Realm.
pub trait RealmAttention {
    /// Get the attention tracking document for this realm.
    async fn attention(&self) -> Result<Document<AttentionDocument>>;

    /// Focus on a specific quest.
    async fn focus_on_intention(
        &self,
        intention_id: IntentionId,
        member: MemberId,
    ) -> Result<AttentionEventId>;

    /// Clear attention (stop focusing on any quest).
    async fn clear_attention(
        &self,
        member: MemberId,
    ) -> Result<AttentionEventId>;

    /// Get current focus for a member.
    async fn get_member_focus(
        &self,
        member: &MemberId,
    ) -> Result<Option<IntentionId>>;

    /// Get all members currently focusing on an intention.
    async fn get_intention_focusers(
        &self,
        intention_id: &IntentionId,
    ) -> Result<Vec<MemberId>>;

    /// Get quests ranked by total attention time.
    async fn intentions_by_attention(
        &self,
    ) -> Result<Vec<IntentionAttention>>;

    /// Get attention details for a specific quest.
    async fn intention_attention(
        &self,
        intention_id: &IntentionId,
    ) -> Result<IntentionAttention>;

    /// Get quests ranked by humanness-weighted attention.
    ///
    /// Each member's attention is multiplied by their humanness freshness
    /// (0.0–1.0). Members without recent attestation contribute zero.
    /// This makes Sybil accounts' attention invisible without banning them.
    async fn quests_by_weighted_attention(&self) -> Result<Vec<WeightedQuestAttention>>;

    /// Get the attention tip document (chain tip advertisements).
    async fn attention_tips(&self) -> Result<Document<AttentionTipDocument>>;

    /// Get the fraud evidence document (equivocation proofs).
    async fn fraud_evidence(&self) -> Result<Document<FraudEvidenceDocument>>;

    /// Get the witness roster document.
    async fn witness_roster(&self) -> Result<Document<WitnessRosterDocument>>;

    /// Get the certificate document.
    async fn certificates(&self) -> Result<Document<CertificateDocument>>;

    /// Create a witness signature for an event.
    ///
    /// Validates the event's PQ signature against the author's public key
    /// before co-signing. Returns an error if the event signature is invalid.
    async fn request_witness_signature(
        &self,
        event: &ChainedSwitchEvent,
        intention_scope: ArtifactId,
        identity: &PQIdentity,
        witness_id: MemberId,
        author_pubkey: &PQPublicIdentity,
    ) -> Result<WitnessSignature>;

    /// Submit a completed quorum certificate for storage and distribution.
    ///
    /// Validates the certificate against the roster and public keys before
    /// storing. The certificate must have at least `k` valid signatures
    /// from roster members. Propagates to peers via CRDT sync.
    async fn submit_certificate(
        &self,
        cert: QuorumCertificate,
        roster: &[indras_artifacts::artifact::PlayerId],
        k: usize,
        public_keys: &std::collections::HashMap<
            indras_artifacts::artifact::PlayerId,
            PQPublicIdentity,
        >,
    ) -> Result<()>;

    /// Create a genesis event for a new author's attention chain.
    ///
    /// This is the first event in an author's chain (seq=0, from=None, prev=zeros).
    /// Signs it with the author's PQ identity, stores it, updates the tip document,
    /// and returns the event along with the initial `AuthorState` for subsequent calls
    /// to `switch_attention_conserved()`.
    async fn create_genesis_event(
        &self,
        to: Option<ArtifactId>,
        author: MemberId,
        identity: &PQIdentity,
    ) -> Result<(ChainedSwitchEvent, AuthorState)>;

    /// Switch attention with conservation guarantees (hash-chained, PQ-signed).
    ///
    /// Creates a new chain event, signs it, stores it in the attention document,
    /// updates the tip document, and returns the event.
    async fn switch_attention_conserved(
        &self,
        from: Option<ArtifactId>,
        to: Option<ArtifactId>,
        author: MemberId,
        identity: &PQIdentity,
        author_state: &mut AuthorState,
    ) -> Result<ChainedSwitchEvent>;
}

/// Ensure a witness roster exists for the given scope.
///
/// If no roster exists yet, populates it from current realm members
/// (excluding the author) and computes the BFT quorum threshold.
/// Returns `(roster_members, k)` for callers that need the threshold.
async fn ensure_witness_roster(
    realm: &Realm,
    scope: &ArtifactId,
    author: MemberId,
) -> Result<()> {
    let roster_doc = realm.document::<WitnessRosterDocument>("witness-roster").await?;

    // Check if roster already exists for this scope
    let has_roster = !roster_doc.read().await.get_roster(scope).is_empty();
    if has_roster {
        return Ok(());
    }

    // Get all realm members, exclude the author
    let members = realm.member_list().await?;
    let witnesses: Vec<MemberId> = members
        .iter()
        .map(|m| m.id())
        .filter(|id| *id != author)
        .collect();

    if witnesses.is_empty() {
        return Ok(()); // Solo participant — no witnesses possible
    }

    // Store the roster
    let scope_clone = *scope;
    roster_doc
        .update(|d| {
            d.set_roster(scope_clone, witnesses);
        })
        .await?;

    Ok(())
}

impl RealmAttention for Realm {
    async fn attention(&self) -> Result<Document<AttentionDocument>> {
        self.document("attention").await
    }

    async fn focus_on_intention(&self, intention_id: IntentionId, member: MemberId) -> Result<AttentionEventId> {
        let mut event_id = [0u8; 16];
        let doc = self.attention().await?;
        doc.update(|d| {
            event_id = d.focus_on_intention(member, intention_id);
        })
        .await?;

        Ok(event_id)
    }

    async fn clear_attention(&self, member: MemberId) -> Result<AttentionEventId> {
        let mut event_id = [0u8; 16];
        let doc = self.attention().await?;
        doc.update(|d| {
            event_id = d.clear_attention(member);
        })
        .await?;

        Ok(event_id)
    }

    async fn get_member_focus(&self, member: &MemberId) -> Result<Option<IntentionId>> {
        let doc = self.attention().await?;
        Ok(doc.read().await.current_focus(member))
    }

    async fn get_intention_focusers(&self, intention_id: &IntentionId) -> Result<Vec<MemberId>> {
        let doc = self.attention().await?;
        Ok(doc.read().await.members_focusing_on(intention_id))
    }

    async fn intentions_by_attention(&self) -> Result<Vec<IntentionAttention>> {
        let doc = self.attention().await?;
        Ok(doc.read().await.intentions_by_attention(None))
    }

    async fn intention_attention(&self, intention_id: &IntentionId) -> Result<IntentionAttention> {
        let doc = self.attention().await?;
        Ok(doc.read().await.intention_attention(intention_id, None))
    }

    async fn quests_by_weighted_attention(&self) -> Result<Vec<WeightedQuestAttention>> {
        let attention_doc = self.attention().await?;
        let raw_rankings = attention_doc.read().await.intentions_by_attention(None);

        let humanness_doc: Document<HumannessDocument> = self.document("_humanness").await?;
        let humanness_guard = humanness_doc.read().await;
        let now = chrono::Utc::now().timestamp_millis();

        let mut weighted: Vec<WeightedQuestAttention> = raw_rankings
            .into_iter()
            .map(|qa| {
                let mut by_member = HashMap::new();
                let mut weighted_total = 0.0_f64;

                for (member, raw_millis) in &qa.attention_by_member {
                    let freshness = humanness_guard.freshness_at(member, now);
                    let w = *raw_millis as f64 * freshness;
                    by_member.insert(*member, (*raw_millis, freshness, w));
                    weighted_total += w;
                }

                WeightedQuestAttention {
                    intention_id: qa.intention_id,
                    raw_attention_millis: qa.total_attention_millis,
                    weighted_attention_millis: weighted_total,
                    by_member,
                }
            })
            .collect();

        // Sort by weighted attention (highest first)
        weighted.sort_by(|a, b| {
            b.weighted_attention_millis
                .partial_cmp(&a.weighted_attention_millis)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        Ok(weighted)
    }

    async fn attention_tips(&self) -> Result<Document<AttentionTipDocument>> {
        self.document("attention-tips").await
    }

    async fn fraud_evidence(&self) -> Result<Document<FraudEvidenceDocument>> {
        self.document("fraud-evidence").await
    }

    async fn witness_roster(&self) -> Result<Document<WitnessRosterDocument>> {
        self.document("witness-roster").await
    }

    async fn certificates(&self) -> Result<Document<CertificateDocument>> {
        self.document("certificates").await
    }

    async fn request_witness_signature(
        &self,
        event: &ChainedSwitchEvent,
        intention_scope: ArtifactId,
        identity: &PQIdentity,
        witness_id: MemberId,
        author_pubkey: &PQPublicIdentity,
    ) -> Result<WitnessSignature> {
        // Validate the event's PQ signature before co-signing
        if !event.verify_signature(author_pubkey) {
            return Err(IndraError::InvalidOperation(
                "witness refused: event has invalid or missing author signature".to_string(),
            ));
        }

        let event_hash = event.event_hash();
        let ws = WitnessSignature::sign(&event_hash, &intention_scope, identity, witness_id);
        Ok(ws)
    }

    async fn submit_certificate(
        &self,
        cert: QuorumCertificate,
        roster: &[indras_artifacts::artifact::PlayerId],
        k: usize,
        public_keys: &std::collections::HashMap<
            indras_artifacts::artifact::PlayerId,
            PQPublicIdentity,
        >,
    ) -> Result<()> {
        // Validate the certificate before storing
        cert.verify(roster, k, public_keys).map_err(|e| {
            IndraError::InvalidOperation(format!("certificate validation failed: {e}"))
        })?;

        let doc = self.certificates().await?;
        doc.update(|d| {
            d.store_certificate(cert);
        })
        .await?;
        Ok(())
    }

    async fn create_genesis_event(
        &self,
        to: Option<ArtifactId>,
        author: MemberId,
        identity: &PQIdentity,
    ) -> Result<(ChainedSwitchEvent, AuthorState)> {
        // Auto-populate witness roster for the target scope
        if let Some(scope) = &to {
            ensure_witness_roster(self, scope, author).await?;
        }

        let now = chrono::Utc::now().timestamp_millis();

        // Create genesis event: seq=0, from=None, prev=zeros
        let mut event = ChainedSwitchEvent::new(author, 0, now, None, to, [0u8; 32]);
        event.sign(identity);

        // Store in attention document
        let doc = self.attention().await?;
        let event_clone = event.clone();
        doc.update(|d| {
            d.store_chain_event(event_clone);
        }).await?;

        // Update tip document
        let tip = AttentionTip {
            author,
            seq: 0,
            event_hash: event.event_hash(),
            wall_time_ms: now,
        };
        let tip_doc = self.attention_tips().await?;
        tip_doc.update(|d| {
            d.update_tip(tip);
        }).await?;

        let author_state = AuthorState {
            latest_seq: 0,
            latest_hash: event.event_hash(),
            current_attention: to,
        };

        Ok((event, author_state))
    }

    async fn switch_attention_conserved(
        &self,
        from: Option<ArtifactId>,
        to: Option<ArtifactId>,
        author: MemberId,
        identity: &PQIdentity,
        author_state: &mut AuthorState,
    ) -> Result<ChainedSwitchEvent> {
        // Auto-populate witness roster for the target scope
        if let Some(scope) = &to {
            ensure_witness_roster(self, scope, author).await?;
        }

        let now = chrono::Utc::now().timestamp_millis();
        let seq = author_state.latest_seq + 1;

        // 1. Create the chain event
        let mut event = ChainedSwitchEvent::new(
            author,
            seq,
            now,
            from,
            to,
            author_state.latest_hash,
        );

        // 2. Sign with PQ identity
        event.sign(identity);

        // 3. Store in attention document (broadcasts via CRDT)
        let doc = self.attention().await?;
        let event_clone = event.clone();
        let mut fraud_record = None;
        doc.update(|d| {
            // Check for equivocation before storing
            if let Some(proof) = d.check_chain_equivocation(&event_clone) {
                let event_a_bytes = postcard::to_allocvec(&proof.event_a).unwrap_or_default();
                let event_b_bytes = postcard::to_allocvec(&proof.event_b).unwrap_or_default();
                fraud_record = Some(FraudRecord {
                    author: proof.author,
                    seq: proof.seq,
                    event_a_bytes,
                    event_b_bytes,
                    reporter: author,
                    detected_at_ms: now,
                });
            }
            d.store_chain_event(event_clone.clone());
        }).await?;

        // 4. Publish fraud if detected
        if let Some(record) = fraud_record {
            let fraud_doc = self.fraud_evidence().await?;
            fraud_doc.update(|d| {
                d.add_record(record);
            }).await?;
        }

        // 5. Update tip document
        let tip = AttentionTip {
            author,
            seq,
            event_hash: event.event_hash(),
            wall_time_ms: now,
        };
        let tip_doc = self.attention_tips().await?;
        tip_doc.update(|d| {
            d.update_tip(tip);
        }).await?;

        // 6. Update caller's author state
        author_state.latest_seq = seq;
        author_state.latest_hash = event.event_hash();
        author_state.current_attention = to;

        Ok(event)
    }
}
