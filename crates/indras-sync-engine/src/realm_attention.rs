//! Extension trait adding attention tracking methods to Realm.

use crate::attention::{AttentionDocument, AttentionEventId, QuestAttention};
use crate::attention_tip::{AttentionTip, AttentionTipDocument};
use crate::fraud_evidence::{FraudEvidenceDocument, FraudRecord};
use crate::quest::QuestId;
use indras_artifacts::attention::AttentionSwitchEvent as ChainedSwitchEvent;
use indras_artifacts::attention::validate::AuthorState;
use indras_artifacts::artifact::ArtifactId;
use indras_crypto::PQIdentity;
use indras_network::document::Document;
use indras_network::error::Result;
use indras_network::member::MemberId;
use indras_network::Realm;

/// Attention tracking extension trait for Realm.
pub trait RealmAttention {
    /// Get the attention tracking document for this realm.
    async fn attention(&self) -> Result<Document<AttentionDocument>>;

    /// Focus on a specific quest.
    async fn focus_on_quest(
        &self,
        quest_id: QuestId,
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
    ) -> Result<Option<QuestId>>;

    /// Get all members currently focusing on a quest.
    async fn get_quest_focusers(
        &self,
        quest_id: &QuestId,
    ) -> Result<Vec<MemberId>>;

    /// Get quests ranked by total attention time.
    async fn quests_by_attention(
        &self,
    ) -> Result<Vec<QuestAttention>>;

    /// Get attention details for a specific quest.
    async fn quest_attention(
        &self,
        quest_id: &QuestId,
    ) -> Result<QuestAttention>;

    /// Get the attention tip document (chain tip advertisements).
    async fn attention_tips(&self) -> Result<Document<AttentionTipDocument>>;

    /// Get the fraud evidence document (equivocation proofs).
    async fn fraud_evidence(&self) -> Result<Document<FraudEvidenceDocument>>;

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

impl RealmAttention for Realm {
    async fn attention(&self) -> Result<Document<AttentionDocument>> {
        self.document("attention").await
    }

    async fn focus_on_quest(&self, quest_id: QuestId, member: MemberId) -> Result<AttentionEventId> {
        let mut event_id = [0u8; 16];
        let doc = self.attention().await?;
        doc.update(|d| {
            event_id = d.focus_on_quest(member, quest_id);
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

    async fn get_member_focus(&self, member: &MemberId) -> Result<Option<QuestId>> {
        let doc = self.attention().await?;
        Ok(doc.read().await.current_focus(member))
    }

    async fn get_quest_focusers(&self, quest_id: &QuestId) -> Result<Vec<MemberId>> {
        let doc = self.attention().await?;
        Ok(doc.read().await.members_focusing_on(quest_id))
    }

    async fn quests_by_attention(&self) -> Result<Vec<QuestAttention>> {
        let doc = self.attention().await?;
        Ok(doc.read().await.quests_by_attention(None))
    }

    async fn quest_attention(&self, quest_id: &QuestId) -> Result<QuestAttention> {
        let doc = self.attention().await?;
        Ok(doc.read().await.quest_attention(quest_id, None))
    }

    async fn attention_tips(&self) -> Result<Document<AttentionTipDocument>> {
        self.document("attention-tips").await
    }

    async fn fraud_evidence(&self) -> Result<Document<FraudEvidenceDocument>> {
        self.document("fraud-evidence").await
    }

    async fn create_genesis_event(
        &self,
        to: Option<ArtifactId>,
        author: MemberId,
        identity: &PQIdentity,
    ) -> Result<(ChainedSwitchEvent, AuthorState)> {
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
