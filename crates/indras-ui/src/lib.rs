//! Shared UI components for Indras Network applications.
//!
//! Provides themes, markdown rendering, file utilities, identity display,
//! and preview overlay components shared between genesis and realm-viewer.

pub mod theme;
pub mod markdown;
pub mod file_utils;
pub mod identity;
pub mod preview;
pub mod contact_invite;
pub mod artifact_display;
pub mod chat;
pub mod identity_row;
pub mod peer_strip;
pub mod heat_display;
pub mod vault_sidebar;
pub mod slash_menu;
pub mod detail_panel;

pub use theme::{Theme, ThemedRoot, ThemeSwitcher, CURRENT_THEME};
pub use markdown::{render_markdown_to_html, is_markdown_file};
pub use file_utils::{load_image_as_data_url, load_text_file_content};
pub use identity::{member_name, short_id, format_duration_millis, member_color_class, member_color_var};
pub use preview::{PreviewFile, PreviewViewMode, PreviewContext, MarkdownPreviewOverlay};
pub use contact_invite::ContactInviteOverlay;
pub use artifact_display::{ArtifactDisplayInfo, ArtifactDisplayStatus, ArtifactGallery};
pub use identity_row::IdentityRow;
pub use peer_strip::{PeerStrip, PeerDisplayInfo};
pub use heat_display::{HeatDot, HeatBar, heat_level};
pub use vault_sidebar::{VaultSidebar, TreeNode};
pub use slash_menu::{SlashMenu, SlashAction};
pub use detail_panel::{DetailPanel, PropertyRow, AudienceMember, HeatEntry, TrailEvent, ReferenceItem, SyncEntry};
pub use chat::ChatPanel;

/// Shared CSS containing design tokens, theme definitions, and base styles.
pub const SHARED_CSS: &str = include_str!("../assets/shared.css");
