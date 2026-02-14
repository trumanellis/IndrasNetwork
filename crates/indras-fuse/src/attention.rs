use indras_artifacts::{ArtifactId, ArtifactStore, AttentionStore, PayloadStore, Vault};
use tracing::debug;

/// Fire a `navigate_to` attention event when a file is opened.
pub fn on_open<A: ArtifactStore, P: PayloadStore, T: AttentionStore>(
    vault: &mut Vault<A, P, T>,
    artifact_id: &ArtifactId,
    now: i64,
) {
    debug!("attention: navigate_to {:?}", artifact_id);
    let _ = vault.navigate_to(artifact_id.clone(), now);
}

/// Fire a `navigate_back` attention event when a file is closed.
pub fn on_release<A: ArtifactStore, P: PayloadStore, T: AttentionStore>(
    vault: &mut Vault<A, P, T>,
    parent_id: &ArtifactId,
    now: i64,
) {
    debug!("attention: navigate_back {:?}", parent_id);
    let _ = vault.navigate_back(parent_id.clone(), now);
}
