//! Shared infrastructure for Indras viewer crates.
//!
//! This crate provides the common building blocks used by `indras-home-viewer`
//! and `indras-realm-viewer`:
//!
//! - [`playback`] — global atomic playback controls (pause, speed, step,
//!   reset, seek, shutdown).  A single module replaces the duplicated
//!   `playback.rs` that previously existed in each viewer crate.
//! - [`stream`] — generic async JSONL line reader.  Each viewer supplies its
//!   own concrete event type; the shared reader handles stdin/file dispatch,
//!   empty-line skipping, and per-line parse-error logging.
//!
//! # What is NOT shared
//!
//! Event types (`HomeRealmEvent`, `StreamEvent`) remain in their respective
//! viewer crates because they are domain-specific and unrelated to each other.
//! The collaboration-viewer uses a tick-based state machine rather than JSONL
//! streaming, so it does not use this crate at all.

pub mod playback;
pub mod stream;
