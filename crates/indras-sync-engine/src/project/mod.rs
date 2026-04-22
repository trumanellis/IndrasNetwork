//! Project snapshot and materialize primitives.
//!
//! A **Project** is a manifest-materialized view of the blob store: a directory
//! on disk whose file contents are all stored in (and retrievable from) a
//! [`BlobStore`], with their positions recorded in a [`PatchManifest`].
//!
//! This module owns two operations:
//!
//! * **Snapshot** ([`snapshot::snapshot_dir`]) — walk a directory, hash every
//!   regular file, store blobs, and return a [`PatchManifest`] describing the
//!   current state.
//! * **Materialize** ([`materialize::materialize_to`]) — given a
//!   [`PatchManifest`] and a destination directory, write every file back to
//!   disk from the blob store.
//!
//! Neither operation requires a live [`Vault`](crate::vault::Vault); they work
//! directly with a [`BlobStore`] reference so they are easy to test in
//! isolation and usable outside the vault context.

pub mod materialize;
pub mod registry;
pub mod snapshot;

pub use materialize::materialize_to;
pub use registry::{ProjectEntry, ProjectId, ProjectRegistry};
pub use snapshot::snapshot_dir;
