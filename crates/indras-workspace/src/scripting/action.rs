//! Semantic UI actions that Lua scripts can dispatch via the ActionBus.

/// All semantic actions the Lua API can trigger.
#[derive(Debug, Clone)]
pub enum Action {
    // Navigation
    ClickSidebar(String),
    ClickTab(String),
    ClickPeerDot(String),
    ClickBreadcrumb(usize),

    // Contact flow
    OpenContacts,
    PasteConnectCode(String),
    ClickConnect,
    CloseOverlay,

    // Messaging
    TypeMessage(String),
    SendMessage,

    // Document editing
    ClickBlock(usize),
    TypeInBlock(usize, String),
    AddBlock(String),

    // Slash menu
    OpenSlashMenu,
    SelectSlashAction(String),

    // Setup / onboarding
    SetDisplayName(String),
    ClickCreateIdentity,

    // Artifacts
    /// Store an artifact in the home realm index.
    StoreArtifact {
        name: String,
        mime: String,
        size: u64,
        lat: Option<f64>,
        lng: Option<f64>,
        /// If set, marks this artifact as received from the named peer.
        from_peer: Option<String>,
    },

    /// Grant a peer access to an artifact (by name) via the real HomeRealm API.
    GrantArtifact {
        artifact_name: String,
        peer_name: String,
    },

    /// Set the user's reference location for the artifact browser.
    SetUserLocation { lat: f64, lng: f64 },

    /// Connect to a peer using their identity URI.
    ConnectToPeer { uri: String },

    // Intention game loop
    SubmitProof { intention_label: String, body: String },
    ReleaseAttention { intention_label: String },
    PledgeToken { intention_label: String, token_label: String },

    // Utility
    Wait(f64),
}

impl Action {
    /// Parse an action name and optional argument from Lua.
    pub fn parse(name: &str, arg: Option<String>) -> Result<Self, String> {
        match name {
            "click_sidebar" => Ok(Action::ClickSidebar(arg.ok_or("click_sidebar requires a label")?)),
            "click_tab" => Ok(Action::ClickTab(arg.ok_or("click_tab requires a tab name")?)),
            "click_peer" => Ok(Action::ClickPeerDot(arg.ok_or("click_peer requires a peer name")?)),
            "click_breadcrumb" => {
                let idx = arg.ok_or("click_breadcrumb requires an index")?
                    .parse::<usize>().map_err(|e| e.to_string())?;
                Ok(Action::ClickBreadcrumb(idx))
            }
            "open_contacts" => Ok(Action::OpenContacts),
            "paste_connect_code" => Ok(Action::PasteConnectCode(arg.ok_or("paste_connect_code requires a code")?)),
            "click_connect" => Ok(Action::ClickConnect),
            "close_overlay" => Ok(Action::CloseOverlay),
            "type_message" => Ok(Action::TypeMessage(arg.ok_or("type_message requires text")?)),
            "send_message" => Ok(Action::SendMessage),
            "click_block" => {
                let idx = arg.ok_or("click_block requires an index")?
                    .parse::<usize>().map_err(|e| e.to_string())?;
                Ok(Action::ClickBlock(idx))
            }
            "type_in_block" => Err("type_in_block requires (index, text) — use indras.type_in_block(idx, text)".into()),
            "add_block" => Ok(Action::AddBlock(arg.ok_or("add_block requires a block type")?)),
            "open_slash_menu" => Ok(Action::OpenSlashMenu),
            "select_slash_action" => Ok(Action::SelectSlashAction(arg.ok_or("select_slash_action requires an action name")?)),
            "set_display_name" => Ok(Action::SetDisplayName(arg.ok_or("set_display_name requires a name")?)),
            "click_create_identity" => Ok(Action::ClickCreateIdentity),
            "wait" => {
                let secs = arg.ok_or("wait requires seconds")?
                    .parse::<f64>().map_err(|e| e.to_string())?;
                Ok(Action::Wait(secs))
            }
            "submit_proof" => {
                let arg = arg.ok_or("submit_proof requires 'intention_label|body'")?;
                let parts: Vec<&str> = arg.splitn(2, '|').collect();
                if parts.len() != 2 {
                    return Err("submit_proof requires 'intention_label|body' format".into());
                }
                Ok(Action::SubmitProof {
                    intention_label: parts[0].to_string(),
                    body: parts[1].to_string(),
                })
            }
            "release_attention" => Ok(Action::ReleaseAttention {
                intention_label: arg.ok_or("release_attention requires an intention label")?,
            }),
            "pledge_token" => {
                let arg = arg.ok_or("pledge_token requires 'intention_label|token_label'")?;
                let parts: Vec<&str> = arg.splitn(2, '|').collect();
                if parts.len() != 2 {
                    return Err("pledge_token requires 'intention_label|token_label' format".into());
                }
                Ok(Action::PledgeToken {
                    intention_label: parts[0].to_string(),
                    token_label: parts[1].to_string(),
                })
            }
            other => Err(format!("Unknown action: {}", other)),
        }
    }
}
