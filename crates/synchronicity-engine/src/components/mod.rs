//! UI components for The Synchronicity Engine.

mod app;
mod welcome;
mod pass_story;
mod loading;
mod home_vault;
mod vault_info_bar;
mod vault_columns;
mod private_column;
mod realm_column;
mod file_item;
mod file_modal;
mod markdown_editor;
mod obsidian;
mod context_menu;
mod status_bar;
mod peer_bar;
mod contact_invite;
mod create_realm;
mod relay_settings;
mod profile_modal;
mod peer_profile_popup;
mod recovery_setup;
mod sync_panel;
mod sync_stage_view;
mod topbar;
mod braid_graph;
mod braid_sparkline;
mod braid_drawer;
mod agent_lane;

pub use app::App;
pub use sync_panel::{SyncOverlay, SyncPanel, SyncPanelRow};
pub use sync_stage_view::SyncStageView;
pub use braid_drawer::BraidDrawer;
pub use braid_sparkline::BraidSparkline;
pub use braid_graph::{BraidGraph, BraidGraphCfg, render_braid_svg};
