//! Braided VCS layered on top of the vault.
//!
//! # Purpose
//!
//! Provides the semantic VCS layer — `Changeset`, `BraidDag`, verification
//! gate, and heal loop — for multi-agent peer-to-peer source development.
//! Unlike the original standalone `indras-braid` crate, this submodule
//! **rides on top of the existing vault infrastructure**: source files live
//! in [`VaultFileDocument`](crate::vault::vault_document::VaultFileDocument),
//! and the braid DAG merely references them by content hash.
//!
//! # Architecture
//!
//! - The **working tree** is the realm's `VaultFileDocument`. There is no
//!   separate `SourceTree`; the vault IS the source tree.
//! - A [`Changeset`] carries a [`SymlinkIndex`]: the full
//!   content-addressed filesystem state at that point, plus an
//!   [`IndexDelta`] of what changed from the parent.
//!   The blobs themselves live in the global content-addressed store.
//! - A [`BraidDag`] is a CRDT document (set-union on `ChangeId`) that
//!   holds the DAG history. It rides the same Automerge sync as every
//!   other `DocumentSchema`.
//! - The [`verification`] runner shells to `cargo` for build/test/clippy;
//!   [`heal`] detects post-merge breakage and emits repair-task descriptions.
//! - [`RealmBraid`](realm_braid::RealmBraid) exposes the gate as a `Realm`
//!   extension trait.
//! - [`gate::LocalRepo`] is a thin orchestration struct used by the
//!   `try_land` flow.
//!
//! # Identity
//!
//! Authorship uses [`crate::vault::vault_file::UserId`] — the same 32-byte
//! identity key the vault already tracks — rather than a distinct `AgentId`.

pub mod agent_braid;
pub mod changeset;
pub mod dag;
pub mod gate;
pub mod heal;
pub mod realm_braid;
pub mod verification;

pub use agent_braid::{AgentBraid, MergeResult, derive_agent_id};
pub use changeset::{ChangeId, Changeset, Evidence, PatchFile, PatchManifest};
pub use dag::{BraidDag, PeerState};
pub use gate::{LocalRepo, TryLandError};
pub use heal::{detect_heal_needed, RepairTask};
pub use realm_braid::{verify_only, RealmBraid};
pub use verification::{run, VerificationFailure, VerificationRequest};
