use indras_artifacts::{
    ArtifactId, InMemoryArtifactStore, InMemoryAttentionStore, InMemoryPayloadStore, PlayerId,
    Vault,
};

/// Which screen the app is displaying.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Screen {
    Dashboard,
    CreateIntention,
    AttachTokens,
    ShareIntention(ArtifactId),
    EnterCode,
    ProviderView(ArtifactId),
    FulfillForm(ArtifactId),
    ReleaseReview(ArtifactId),
    ExchangeComplete(ArtifactId),
}

/// View model for a token in the vault.
#[derive(Clone, Debug)]
pub struct TokenView {
    pub id: ArtifactId,
    pub name: String,
    pub description: String,
    pub hours: String,
    pub earned_date: String,
    pub selected: bool,
}

/// View model for a request (intention).
#[derive(Clone, Debug)]
pub struct RequestView {
    pub id: ArtifactId,
    pub title: String,
    pub description: String,
    pub location: String,
    pub token_name: Option<String>,
    pub magic_code: Option<String>,
}

/// View model for an active exchange.
#[derive(Clone, Debug)]
pub struct ExchangeView {
    pub id: ArtifactId,
    pub request_title: String,
    pub provider_name: String,
    pub proof_title: Option<String>,
    pub proof_description: Option<String>,
    pub token_name: String,
    pub completed: bool,
}

/// Central application state.
pub struct ExchangeState {
    // Identity
    pub player_id: PlayerId,
    pub display_name: String,

    // Navigation
    pub screen: Screen,

    // Domain
    pub vault: Vault<InMemoryArtifactStore, InMemoryPayloadStore, InMemoryAttentionStore>,

    // Create intention flow
    pub draft_title: String,
    pub draft_description: String,
    pub draft_location: String,
    pub selected_token_ids: Vec<ArtifactId>,

    // View models
    pub tokens: Vec<TokenView>,
    pub active_requests: Vec<RequestView>,
    pub active_exchanges: Vec<ExchangeView>,
    pub completed_exchanges: Vec<ExchangeView>,

    // Provider flow
    pub incoming_request: Option<RequestView>,
    pub draft_proof_title: String,
    pub draft_proof_description: String,

    // Encounter
    pub pending_encounter_code: Option<String>,
    pub code_input: String,

    // Status
    pub status_message: Option<String>,
}

impl ExchangeState {
    pub fn new() -> Self {
        let player_id: PlayerId = rand::random();
        let now = chrono::Utc::now().timestamp();
        let vault = Vault::in_memory(player_id, now).expect("Failed to create vault");

        Self {
            player_id,
            display_name: String::new(),
            screen: Screen::Dashboard,
            vault,
            draft_title: String::new(),
            draft_description: String::new(),
            draft_location: String::new(),
            selected_token_ids: Vec::new(),
            tokens: Vec::new(),
            active_requests: Vec::new(),
            active_exchanges: Vec::new(),
            completed_exchanges: Vec::new(),
            incoming_request: None,
            draft_proof_title: String::new(),
            draft_proof_description: String::new(),
            pending_encounter_code: None,
            code_input: String::new(),
            status_message: None,
        }
    }
}
