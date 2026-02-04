//! Extension trait adding humanness attestation methods to Realm.

use crate::humanness::HumannessDocument;
use indras_network::document::Document;
use indras_network::error::Result;
use indras_network::escape::PeerIdentity;
use indras_network::member::MemberId;
use indras_network::Realm;

/// Humanness attestation extension trait for Realm.
pub trait RealmHumanness {
    /// Get the humanness document for this realm.
    async fn humanness(&self) -> Result<Document<HumannessDocument>>;

    /// Record a proof of life celebration.
    async fn record_proof_of_life(
        &self,
        participants: Vec<MemberId>,
    ) -> Result<()>;

    /// Get the humanness freshness for a specific member.
    async fn humanness_freshness_for(
        &self,
        member: &MemberId,
    ) -> Result<f64>;
}

impl RealmHumanness for Realm {
    async fn humanness(&self) -> Result<Document<HumannessDocument>> {
        self.document("_humanness").await
    }

    async fn record_proof_of_life(&self, participants: Vec<MemberId>) -> Result<()> {
        let humanness_doc = self.humanness().await?;
        let attester: MemberId = self.node().identity().as_bytes().try_into().expect("identity bytes");
        let timestamp = chrono::Utc::now().timestamp_millis();

        humanness_doc
            .update(|d| {
                d.record_proof_of_life(participants.clone(), attester, timestamp);
            })
            .await?;

        Ok(())
    }

    async fn humanness_freshness_for(&self, member: &MemberId) -> Result<f64> {
        let humanness_doc = self.humanness().await?;
        let guard = humanness_doc.read().await;
        let now = chrono::Utc::now().timestamp_millis();
        Ok(guard.freshness_at(member, now))
    }
}
