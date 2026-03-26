//! Core application state for the Gift Cycle app.
//!
//! Defines the cycle stages, navigation views, and shared state
//! that drives the UI. All state is held in Dioxus signals.

use indras_network::member::MemberId;
use indras_sync_engine::IntentionId;

/// The six stages of the gift economy cycle.
#[derive(Clone, Debug, PartialEq)]
pub enum CycleStage {
    /// Someone voices a need or offering.
    Intention,
    /// Peers invest attention (dwell time).
    Attention,
    /// A peer performs an act of service.
    Service,
    /// The creator blesses the work.
    Blessing,
    /// Gratitude crystallizes into a token.
    Token,
    /// Tokens feed new intentions.
    Renewal,
}

impl CycleStage {
    /// Emoji icon for this stage.
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Intention => "\u{1f4a1}",
            Self::Attention => "\u{1f441}",
            Self::Service => "\u{1f932}",
            Self::Blessing => "\u{2728}",
            Self::Token => "\u{1fa99}",
            Self::Renewal => "\u{1f504}",
        }
    }

    /// Display label for this stage.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Intention => "Intention",
            Self::Attention => "Attention",
            Self::Service => "Act of Service",
            Self::Blessing => "Blessing",
            Self::Token => "Token",
            Self::Renewal => "Renewal",
        }
    }

    /// CSS color variable name for this stage.
    pub fn color_var(&self) -> &'static str {
        match self {
            Self::Intention => "var(--stage-intention)",
            Self::Attention => "var(--stage-attention)",
            Self::Service => "var(--stage-service)",
            Self::Blessing => "var(--stage-blessing)",
            Self::Token => "var(--stage-token)",
            Self::Renewal => "var(--stage-renewal)",
        }
    }

    /// All six stages in cycle order.
    pub fn all() -> [CycleStage; 6] {
        [
            Self::Intention,
            Self::Attention,
            Self::Service,
            Self::Blessing,
            Self::Token,
            Self::Renewal,
        ]
    }
}

/// Which view the app is currently showing.
#[derive(Clone, Debug, PartialEq)]
pub enum AppView {
    /// Intention feed (home screen).
    Feed,
    /// Full detail for a single intention.
    Detail(IntentionId),
    /// Create a new intention.
    CreateIntention,
    /// Submit proof of service for an intention.
    SubmitProof(IntentionId),
    /// Review a claim and bless it.
    Bless(IntentionId, MemberId),
    /// Token wallet.
    Wallet,
    /// Profile visibility / grant management.
    Profile,
    /// Relay node dashboard.
    RelayDashboard,
}

// AppState is not used currently — state lives in signals in app.rs.
// Keeping the types above (CycleStage, AppView) which are used by components.
