use crate::artifact::*;
use crate::attention::{extract_dwell_windows, DwellWindow};
use crate::error::VaultError;
use crate::store::{ArtifactStore, AttentionStore, PayloadStore};
use crate::vault::Vault;

type Result<T> = std::result::Result<T, VaultError>;

const DESCRIPTION_LABEL: &str = "description";
const PROOF_PREFIX: &str = "proof:";
const PLEDGE_PREFIX: &str = "pledge:";
const STATUS_KEY: &str = "status";
const RELEASED_PREFIX: &str = "released:";

/// An Intention is an Artifact (artifact_type = "intention") representing a player's
/// stated goal that can be charged with collective attention, receive proofs of
/// service, and release stored attention as tokens of gratitude.
///
/// ## Structure (labeled refs)
///
/// ```text
/// Intention artifact (artifact_type = "intention")
/// ├── ref label="description"                    → message artifact (description text)
/// ├── ref label="proof:{submitter_hex}:{ts}"     → attestation artifact (proof body)
/// ├── ref label="pledge:{from_hex}:{token_hex}"  → token artifact (pledged by supporter)
/// └── metadata:
///     ├── "status"                               → b"active" | b"fulfilled"
///     └── "released:{player_hex}:{start_ts}"     → b"{proof_label}" (prevents double-release)
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Intention {
    pub id: ArtifactId,
}

impl Intention {
    /// Wrap an existing ArtifactId as an Intention.
    pub fn from_id(id: ArtifactId) -> Self {
        Self { id }
    }

    /// Create a new Intention tree with a description message.
    pub fn create<A: ArtifactStore, P: PayloadStore, T: AttentionStore>(
        vault: &mut Vault<A, P, T>,
        description_text: &str,
        audience: Vec<PlayerId>,
        now: i64,
    ) -> Result<Self> {
        let tree = vault.place_tree("intention", audience, now)?;
        let intention_id = tree.id;

        // Create description as a Leaf(Message) child
        let desc_leaf = vault.place_leaf(
            description_text.as_bytes(),
            "description".to_string(),
            None,
            "message",
            now,
        )?;
        vault.compose(
            &intention_id,
            desc_leaf.id,
            0,
            Some(DESCRIPTION_LABEL.to_string()),
        )?;

        // Set initial status to "active"
        let mut artifact = vault
            .get_artifact(&intention_id)?
            .ok_or(VaultError::ArtifactNotFound)?;
        artifact.metadata
            .insert(STATUS_KEY.to_string(), b"active".to_vec());
        vault.artifact_store_mut().put_artifact(&artifact)?;

        Ok(Self { id: intention_id })
    }

    /// Get the description artifact.
    pub fn description<A: ArtifactStore, P: PayloadStore, T: AttentionStore>(
        &self,
        vault: &Vault<A, P, T>,
    ) -> Result<Option<Artifact>> {
        let refs = self.get_refs(vault)?;
        let desc_ref = refs
            .iter()
            .find(|r| r.label.as_deref() == Some(DESCRIPTION_LABEL));
        match desc_ref {
            Some(r) => vault.get_artifact(&r.artifact_id),
            None => Ok(None),
        }
    }

    /// Submit a proof of service as an Attestation leaf.
    pub fn submit_proof<A: ArtifactStore, P: PayloadStore, T: AttentionStore>(
        &self,
        vault: &mut Vault<A, P, T>,
        body: &str,
        now: i64,
    ) -> Result<ArtifactId> {
        let submitter = *vault.player();
        let submitter_hex: String = submitter.iter().map(|b| format!("{b:02x}")).collect();

        let proof_leaf = vault.place_leaf(
            body.as_bytes(),
            format!("proof-{submitter_hex}"),
            None,
            "attestation",
            now,
        )?;

        let next_pos = self.ref_count(vault)? as u64;
        let label = format!("{PROOF_PREFIX}{submitter_hex}:{now}");
        vault.compose(&self.id, proof_leaf.id, next_pos, Some(label))?;

        Ok(proof_leaf.id)
    }

    /// Submit an existing Collection tree (folder) as a proof reference.
    pub fn submit_proof_folder<A: ArtifactStore, P: PayloadStore, T: AttentionStore>(
        &self,
        vault: &mut Vault<A, P, T>,
        folder_id: ArtifactId,
        now: i64,
    ) -> Result<()> {
        let submitter = *vault.player();
        let submitter_hex: String = submitter.iter().map(|b| format!("{b:02x}")).collect();

        let next_pos = self.ref_count(vault)? as u64;
        let label = format!("{PROOF_PREFIX}{submitter_hex}:{now}");
        vault.compose(&self.id, folder_id, next_pos, Some(label))
    }

    /// List all proof refs (filter by "proof:" prefix).
    pub fn proofs<A: ArtifactStore, P: PayloadStore, T: AttentionStore>(
        &self,
        vault: &Vault<A, P, T>,
    ) -> Result<Vec<ArtifactRef>> {
        let refs = self.get_refs(vault)?;
        Ok(refs
            .into_iter()
            .filter(|r| {
                r.label
                    .as_deref()
                    .map_or(false, |l| l.starts_with(PROOF_PREFIX))
            })
            .collect())
    }

    /// Pledge a token to this intention (attach as labeled ref).
    pub fn pledge_token<A: ArtifactStore, P: PayloadStore, T: AttentionStore>(
        &self,
        vault: &mut Vault<A, P, T>,
        token_id: ArtifactId,
    ) -> Result<()> {
        let from = *vault.player();
        let from_hex: String = from.iter().map(|b| format!("{b:02x}")).collect();
        let token_hex: String = token_id
            .bytes()
            .iter()
            .take(8)
            .map(|b| format!("{b:02x}"))
            .collect();

        let next_pos = self.ref_count(vault)? as u64;
        let label = format!("{PLEDGE_PREFIX}{from_hex}:{token_hex}");
        vault.compose(&self.id, token_id, next_pos, Some(label))
    }

    /// List all pledged token refs (filter by "pledge:" prefix).
    pub fn pledged_tokens<A: ArtifactStore, P: PayloadStore, T: AttentionStore>(
        &self,
        vault: &Vault<A, P, T>,
    ) -> Result<Vec<ArtifactRef>> {
        let refs = self.get_refs(vault)?;
        Ok(refs
            .into_iter()
            .filter(|r| {
                r.label
                    .as_deref()
                    .map_or(false, |l| l.starts_with(PLEDGE_PREFIX))
            })
            .collect())
    }

    /// Compute unreleased attention windows for a player on this intention.
    ///
    /// 1. Gets player's own events + peer events for that player
    /// 2. Extracts dwell windows where attention was on this intention
    /// 3. Filters out already-released windows (matched by start_timestamp in metadata)
    pub fn unreleased_attention<A: ArtifactStore, P: PayloadStore, T: AttentionStore>(
        &self,
        vault: &Vault<A, P, T>,
        player: PlayerId,
    ) -> Result<Vec<DwellWindow>> {
        // Gather events: own log if player is vault owner, otherwise peer log
        let events = if player == *vault.player() {
            vault.attention_events()?
        } else {
            vault
                .peer_attention()
                .get(&player)
                .cloned()
                .unwrap_or_default()
        };

        // Extract dwell windows for this intention
        let windows = extract_dwell_windows(player, &self.id, &events);

        // Read metadata to find already-released windows
        let artifact = vault
            .get_artifact(&self.id)?
            .ok_or(VaultError::ArtifactNotFound)?;

        let player_hex: String = player.iter().map(|b| format!("{b:02x}")).collect();
        let released_prefix = format!("{RELEASED_PREFIX}{player_hex}:");

        let released_timestamps: Vec<i64> = artifact
            .metadata
            .keys()
            .filter_map(|k: &String| {
                if let Some(ts_str) = k.strip_prefix(&released_prefix) {
                    ts_str.parse::<i64>().ok()
                } else {
                    None
                }
            })
            .collect();

        // Filter out already-released windows
        Ok(windows
            .into_iter()
            .filter(|w| !released_timestamps.contains(&w.start_timestamp))
            .collect())
    }

    /// Release attention windows as tokens, transferring stewardship to proof submitter.
    ///
    /// For each selected DwellWindow:
    /// 1. Create Token leaf from duration
    /// 2. Add BlessingRecord to the token
    /// 3. Transfer stewardship to proof_submitter
    /// 4. Record in Intention metadata to prevent double-release
    pub fn release_attention<A: ArtifactStore, P: PayloadStore, T: AttentionStore>(
        &self,
        vault: &mut Vault<A, P, T>,
        windows: &[DwellWindow],
        proof_submitter: PlayerId,
        now: i64,
    ) -> Result<Vec<ArtifactId>> {
        let player = *vault.player();
        let player_hex: String = player.iter().map(|b| format!("{b:02x}")).collect();
        let mut created_tokens = Vec::new();

        for window in windows {
            // 1. Create Token leaf from duration
            let duration_bytes = window.duration_ms.to_le_bytes();
            let token_name = format!(
                "token-{}ms-{}",
                window.duration_ms, window.start_timestamp
            );
            let token_leaf = vault.place_leaf(
                &duration_bytes,
                token_name,
                None,
                "token",
                now,
            )?;
            let token_id = token_leaf.id;

            // 2. Add BlessingRecord to the token (get-modify-put)
            let mut token_artifact = vault
                .get_artifact(&token_id)?
                .ok_or(VaultError::ArtifactNotFound)?;
            token_artifact.blessing_history.push(BlessingRecord {
                from: player,
                quest_id: Some(self.id),
                timestamp: now,
                message: None,
            });
            vault.artifact_store_mut().put_artifact(&token_artifact)?;

            // 3. Transfer stewardship to proof_submitter
            vault.transfer_stewardship(&token_id, proof_submitter, now)?;

            // 4. Record release in Intention metadata (get-modify-put)
            let key = format!(
                "{RELEASED_PREFIX}{player_hex}:{}",
                window.start_timestamp
            );
            let proof_label = format!("{PROOF_PREFIX}{}", {
                let hex: String = proof_submitter
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect();
                hex
            });
            let mut intention_artifact = vault
                .get_artifact(&self.id)?
                .ok_or(VaultError::ArtifactNotFound)?;
            intention_artifact.metadata.insert(key, proof_label.into_bytes());
            vault
                .artifact_store_mut()
                .put_artifact(&intention_artifact)?;

            created_tokens.push(token_id);
        }

        Ok(created_tokens)
    }

    /// Release pledged tokens to a proof submitter by transferring stewardship.
    pub fn release_pledged_tokens<A: ArtifactStore, P: PayloadStore, T: AttentionStore>(
        &self,
        vault: &mut Vault<A, P, T>,
        token_ids: &[ArtifactId],
        proof_submitter: PlayerId,
        now: i64,
    ) -> Result<()> {
        for token_id in token_ids {
            // Transfer stewardship
            vault.transfer_stewardship(token_id, proof_submitter, now)?;

            // Remove pledge ref from intention
            vault.remove_ref(&self.id, token_id)?;
        }
        Ok(())
    }

    /// Read the current status of this intention.
    pub fn status<A: ArtifactStore, P: PayloadStore, T: AttentionStore>(
        &self,
        vault: &Vault<A, P, T>,
    ) -> Result<Option<String>> {
        let artifact = vault
            .get_artifact(&self.id)?
            .ok_or(VaultError::ArtifactNotFound)?;
        Ok(artifact
            .metadata
            .get(STATUS_KEY)
            .and_then(|v| String::from_utf8(v.clone()).ok()))
    }

    /// Mark this intention as fulfilled (get-modify-put).
    pub fn fulfill<A: ArtifactStore, P: PayloadStore, T: AttentionStore>(
        &self,
        vault: &mut Vault<A, P, T>,
    ) -> Result<()> {
        let mut artifact = vault
            .get_artifact(&self.id)?
            .ok_or(VaultError::ArtifactNotFound)?;
        artifact.metadata
            .insert(STATUS_KEY.to_string(), b"fulfilled".to_vec());
        vault.artifact_store_mut().put_artifact(&artifact)?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    fn get_refs<A: ArtifactStore, P: PayloadStore, T: AttentionStore>(
        &self,
        vault: &Vault<A, P, T>,
    ) -> Result<Vec<ArtifactRef>> {
        let artifact = vault
            .get_artifact(&self.id)?
            .ok_or(VaultError::ArtifactNotFound)?;
        Ok(artifact.references.clone())
    }

    fn ref_count<A: ArtifactStore, P: PayloadStore, T: AttentionStore>(
        &self,
        vault: &Vault<A, P, T>,
    ) -> Result<usize> {
        let artifact = vault
            .get_artifact(&self.id)?
            .ok_or(VaultError::ArtifactNotFound)?;
        Ok(artifact.references.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::Vault;

    fn player_a() -> PlayerId {
        [1u8; 32]
    }
    fn player_b() -> PlayerId {
        [2u8; 32]
    }
    fn player_c() -> PlayerId {
        [3u8; 32]
    }

    #[test]
    fn test_create_intention() {
        let mut vault = Vault::in_memory(player_a(), 1000).unwrap();
        let intention =
            Intention::create(&mut vault, "Learn Rust basics", vec![player_a(), player_b()], 1000)
                .unwrap();

        // Should have a description
        let desc = intention.description(&vault).unwrap();
        assert!(desc.is_some());

        // Status should be active
        let status = intention.status(&vault).unwrap();
        assert_eq!(status, Some("active".to_string()));

        // No proofs yet
        let proofs = intention.proofs(&vault).unwrap();
        assert!(proofs.is_empty());
    }

    #[test]
    fn test_submit_proof() {
        let mut vault = Vault::in_memory(player_a(), 1000).unwrap();
        let intention =
            Intention::create(&mut vault, "Build relay server", vec![player_a(), player_b()], 1000)
                .unwrap();

        let proof_id = intention
            .submit_proof(&mut vault, "Completed the relay implementation", 2000)
            .unwrap();

        let proofs = intention.proofs(&vault).unwrap();
        assert_eq!(proofs.len(), 1);
        assert_eq!(proofs[0].artifact_id, proof_id);
        assert!(proofs[0]
            .label
            .as_ref()
            .unwrap()
            .starts_with("proof:"));
    }

    #[test]
    fn test_pledge_token() {
        let mut vault = Vault::in_memory(player_a(), 1000).unwrap();
        let intention =
            Intention::create(&mut vault, "Write documentation", vec![player_a()], 1000).unwrap();

        // Create a token to pledge
        let token = vault
            .place_leaf(b"token-data", "my-token".to_string(), None, "token", 1000)
            .unwrap();

        intention.pledge_token(&mut vault, token.id).unwrap();

        let pledged = intention.pledged_tokens(&vault).unwrap();
        assert_eq!(pledged.len(), 1);
        assert_eq!(pledged[0].artifact_id, token.id);
        assert!(pledged[0]
            .label
            .as_ref()
            .unwrap()
            .starts_with("pledge:"));
    }

    #[test]
    fn test_unreleased_attention_and_release() {
        let mut vault = Vault::in_memory(player_a(), 1000).unwrap();
        let intention = Intention::create(
            &mut vault,
            "Design the UI",
            vec![player_a(), player_b(), player_c()],
            1000,
        )
        .unwrap();

        // Player A navigates to the intention (creates attention events)
        vault.navigate_to(intention.id, 2000).unwrap();
        // Navigate away after 5 seconds
        let other_id = vault.place_tree("story", vec![player_a()], 1000).unwrap().id;
        vault.navigate_to(other_id, 7000).unwrap();
        // Navigate back for another 3 seconds
        vault.navigate_to(intention.id, 8000).unwrap();
        vault.navigate_to(other_id, 11000).unwrap();

        // Check unreleased attention
        let windows = intention
            .unreleased_attention(&vault, player_a())
            .unwrap();
        assert_eq!(windows.len(), 2);
        assert_eq!(windows[0].duration_ms, 5000);
        assert_eq!(windows[1].duration_ms, 3000);

        // Player C submits proof
        // We need a separate vault for C since transfer_stewardship requires steward
        // But in this test we stay as A who is releasing their own attention
        // submit_proof is called by whoever submits (in this case, vault owner = A acting on behalf)
        let _proof_id = intention
            .submit_proof(&mut vault, "Designed mockups", 12000)
            .unwrap();

        // Release first window only
        let released_tokens = intention
            .release_attention(&mut vault, &windows[0..1], player_c(), 13000)
            .unwrap();
        assert_eq!(released_tokens.len(), 1);

        // Now unreleased should only have 1 window
        let remaining = intention
            .unreleased_attention(&vault, player_a())
            .unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].duration_ms, 3000);

        // Verify token stewardship was transferred to C
        let token = vault.get_artifact(&released_tokens[0]).unwrap().unwrap();
        assert_eq!(token.steward, player_c());

        // Verify blessing record on token
        assert_eq!(token.blessing_history.len(), 1);
        assert_eq!(token.blessing_history[0].from, player_a());
        assert_eq!(token.blessing_history[0].quest_id, Some(intention.id));
    }

    #[test]
    fn test_no_double_release() {
        let mut vault = Vault::in_memory(player_a(), 1000).unwrap();
        let intention = Intention::create(
            &mut vault,
            "Test double release",
            vec![player_a()],
            1000,
        )
        .unwrap();

        // Create attention
        vault.navigate_to(intention.id, 2000).unwrap();
        let other_id = vault.place_tree("story", vec![player_a()], 1000).unwrap().id;
        vault.navigate_to(other_id, 5000).unwrap();

        let windows = intention
            .unreleased_attention(&vault, player_a())
            .unwrap();
        assert_eq!(windows.len(), 1);

        // Release
        intention
            .release_attention(&mut vault, &windows, player_b(), 6000)
            .unwrap();

        // Should be empty now
        let remaining = intention
            .unreleased_attention(&vault, player_a())
            .unwrap();
        assert!(remaining.is_empty());
    }

    #[test]
    fn test_fulfill_intention() {
        let mut vault = Vault::in_memory(player_a(), 1000).unwrap();
        let intention =
            Intention::create(&mut vault, "Finish project", vec![player_a()], 1000).unwrap();

        assert_eq!(
            intention.status(&vault).unwrap(),
            Some("active".to_string())
        );

        intention.fulfill(&mut vault).unwrap();

        assert_eq!(
            intention.status(&vault).unwrap(),
            Some("fulfilled".to_string())
        );
    }

    #[test]
    fn test_release_pledged_tokens() {
        let mut vault = Vault::in_memory(player_a(), 1000).unwrap();
        let intention =
            Intention::create(&mut vault, "Test pledges", vec![player_a()], 1000).unwrap();

        // Create and pledge a token
        let token = vault
            .place_leaf(b"pledged-token", "pledge-tok".to_string(), None, "token", 1000)
            .unwrap();
        intention.pledge_token(&mut vault, token.id).unwrap();

        assert_eq!(intention.pledged_tokens(&vault).unwrap().len(), 1);

        // Release pledged tokens to player B
        intention
            .release_pledged_tokens(&mut vault, &[token.id], player_b(), 2000)
            .unwrap();

        // Pledge ref should be removed
        assert_eq!(intention.pledged_tokens(&vault).unwrap().len(), 0);

        // Token stewardship should be transferred
        let updated_token = vault.get_artifact(&token.id).unwrap().unwrap();
        assert_eq!(updated_token.steward, player_b());
    }

    #[test]
    fn test_submit_proof_folder() {
        let mut vault = Vault::in_memory(player_a(), 1000).unwrap();
        let intention =
            Intention::create(&mut vault, "Test folders", vec![player_a()], 1000).unwrap();

        // Create a Collection tree to use as proof folder
        let folder = vault
            .place_tree("collection", vec![player_a()], 1000)
            .unwrap();

        intention
            .submit_proof_folder(&mut vault, folder.id, 2000)
            .unwrap();

        let proofs = intention.proofs(&vault).unwrap();
        assert_eq!(proofs.len(), 1);
        assert_eq!(proofs[0].artifact_id, folder.id);
    }
}
