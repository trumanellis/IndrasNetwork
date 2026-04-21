//! The Synchronicity Engine — a sovereign vault synced across all your devices.
//!
//! This is the main desktop application for Indra's Network personal vault sync.
//! On first run, it guides users through account creation with a Pass Story.
//! After that, it presents the home vault — a folder of markdown files that
//! automatically sync across devices via the peer-to-peer network.

pub mod admin_client;
pub mod agent_hooks;
pub mod braid_bridge;
pub mod components;
pub mod config;
pub mod heartbeat;
pub mod ipc;
pub mod profile_bridge;
pub mod profile_mirror;
pub mod profile_server;
pub mod recovery_bridge;
pub mod state;
pub mod team;
pub mod vault_bridge;
pub mod vault_manager;
