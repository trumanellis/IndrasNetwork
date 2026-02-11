use thiserror::Error;

#[derive(Debug, Error)]
pub enum VaultError {
    #[error("not the steward of this artifact")]
    NotSteward,
    #[error("artifact not found")]
    ArtifactNotFound,
    #[error("artifact is not a tree")]
    NotATree,
    #[error("already peered with this player")]
    AlreadyPeered,
    #[error("not peered with this player")]
    NotPeered,
    #[error("payload not loaded (fetch it first)")]
    PayloadNotLoaded,
    #[error("exchange not fully accepted by both parties")]
    ExchangeNotFullyAccepted,
    #[error("store error: {0}")]
    StoreError(String),
}
