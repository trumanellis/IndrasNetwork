//! Vault bridge — connects indras-artifacts Vault to Dioxus signals.

use std::sync::Arc;
use tokio::sync::Mutex;

use indras_artifacts::{
    Vault, InMemoryArtifactStore, InMemoryPayloadStore, InMemoryAttentionStore,
    TreeType, LeafType, PlayerId,
};

/// Type alias for the in-memory vault used in the workspace.
pub type InMemoryVault = Vault<InMemoryArtifactStore, InMemoryPayloadStore, InMemoryAttentionStore>;

/// Handle to the vault, shared across components via Dioxus context.
#[derive(Clone)]
pub struct VaultHandle {
    pub vault: Arc<Mutex<InMemoryVault>>,
    pub player_id: PlayerId,
    pub player_name: String,
}

/// Peer seed data.
pub struct SeedPeer {
    pub name: String,
    pub player_id: PlayerId,
}

/// Create seed peers for the demo workspace.
fn seed_peers() -> Vec<SeedPeer> {
    vec![
        SeedPeer {
            name: "Sage".to_string(),
            player_id: [2u8; 32],
        },
        SeedPeer {
            name: "Zephyr".to_string(),
            player_id: [3u8; 32],
        },
    ]
}

/// Create and seed a vault with sample data matching the mockup.
pub fn create_seeded_vault() -> Result<VaultHandle, indras_artifacts::VaultError> {
    let player_id: PlayerId = [1u8; 32];
    let now = chrono::Utc::now().timestamp_millis();

    let mut vault = InMemoryVault::in_memory(player_id, now)?;
    let root_id = vault.root.id.clone();

    // Add peers
    let peers = seed_peers();
    for peer in &peers {
        vault.peer(peer.player_id, Some(peer.name.clone()), now)?;
    }

    let all_audience = vec![player_id, peers[0].player_id, peers[1].player_id];

    // === Project Alpha folder ===
    let project_alpha = vault.place_tree(TreeType::Document, all_audience.clone(), now)?;
    let project_alpha_id = project_alpha.id.clone();
    vault.compose(&root_id, project_alpha_id.clone(), 0, Some("Project Alpha".to_string()))?;

    // Architecture Notes (Document)
    let arch_notes = vault.place_tree(TreeType::Document, all_audience.clone(), now)?;
    let arch_notes_id = arch_notes.id.clone();
    vault.compose(&project_alpha_id, arch_notes_id.clone(), 0, Some("Architecture Notes".to_string()))?;

    // Add blocks to Architecture Notes
    let blocks: Vec<(&str, &str)> = vec![
        ("text", "The new ontology reduces everything to three primitives: Artifact (Leaf/Tree), Attention Switch Event, and Mutual Peering."),
        ("heading:2", "Core Data Structures"),
        ("text", "Every piece of content is an Artifact. Immutable content lives in Leaf artifacts. Mutable structure lives in Tree artifacts."),
        ("code:rust", "pub enum Artifact {\n    Leaf(LeafArtifact),\n    Tree(TreeArtifact),\n}"),
        ("heading:2", "Navigation as Attention"),
        ("text", "Every click generates an AttentionSwitchEvent. The attention log is append-only and shared with mutual peers."),
        ("callout", "Key insight: The fractal artifact tree IS the UI. Navigation IS attention tracking."),
        ("heading:2", "Tasks"),
        ("todo:done", "Define Artifact enum (Leaf/Tree)"),
        ("todo:done", "Implement Vault with in-memory stores"),
        ("todo", "Wire up AttentionSwitchEvent on navigate_to"),
        ("todo", "Compute heat from peer attention logs"),
        ("divider", ""),
    ];

    for (i, (label, content)) in blocks.iter().enumerate() {
        let leaf = vault.place_leaf(content.as_bytes(), LeafType::Message, now)?;
        vault.compose(&arch_notes_id, leaf.id, i as u64, Some(label.to_string()))?;
    }

    // Team Discussion (Story)
    let team_discussion = vault.place_tree(TreeType::Story, all_audience.clone(), now)?;
    let team_discussion_id = team_discussion.id.clone();
    vault.compose(&project_alpha_id, team_discussion_id.clone(), 1, Some("Team Discussion".to_string()))?;

    // Story messages — labels encode sender + optional richness metadata
    // Format: "msg:Name" or "msg:Name:artifact:ArtifactName" or "msg:Name:image" or "msg:Name:branch:Label"
    let messages: Vec<(PlayerId, &str, &str)> = vec![
        (peers[0].player_id, "Has anyone looked at the new ontology plan? I think collapsing everything into Leaf/Tree is elegant but I'm worried about the migration path from the old Realm model.", "msg:Sage:day:Yesterday"),
        (player_id, "Yeah I've been going through it. The key insight is that Realm members just become per-artifact audiences. So we don't need to migrate \u{2014} we just reinterpret existing data.", "msg:Nova:artifact:Architecture Notes:Tree(Document)"),
        (peers[1].player_id, "Love the heat concept. Being able to see where everyone's attention is focused in real-time makes the workspace feel alive.", "msg:Zephyr"),
        (player_id, "Exactly. And it's not opt-in tracking \u{2014} navigation IS the attention event. Just using the workspace generates the data.", "msg:Nova"),
        (peers[0].player_id, "One question \u{2014} when someone posts a Proof of Service to a Quest, how does the token assignment work? Do you pick specific attention artifacts from your log?", "msg:Sage"),
        (player_id, "Yes \u{2014} every attention switch event is an artifact whose value is the dwell time until the next switch. You browse your log and select specific ones to assign as Tokens of Gratitude linked to that proof. The tokens carry the attention time as their value.", "msg:Nova"),
        (peers[1].player_id, "I pushed an image of the system diagram. Can someone review?", "msg:Zephyr:image:day:Today"),
        (peers[0].player_id, "Looks good! Should we branch this into a design review thread?", "msg:Sage:branch:Branch into sub-Story"),
    ];

    for (i, (_sender, content, label)) in messages.iter().enumerate() {
        let leaf = vault.place_leaf(content.as_bytes(), LeafType::Message, now + (i as i64 * 60_000))?;
        vault.compose(&team_discussion_id, leaf.id, i as u64, Some(label.to_string()))?;
    }

    // Design Assets (Document)
    let design_assets = vault.place_tree(TreeType::Document, all_audience.clone(), now)?;
    vault.compose(&project_alpha_id, design_assets.id.clone(), 2, Some("Design Assets".to_string()))?;

    // === Top-level items ===

    // Personal Journal (Document)
    let journal = vault.place_tree(TreeType::Document, vec![player_id], now)?;
    vault.compose(&root_id, journal.id.clone(), 1, Some("Personal Journal".to_string()))?;

    // DM with Sage (Story)
    let dm_sage = vault.place_tree(TreeType::Story, vec![player_id, peers[0].player_id], now)?;
    vault.compose(&root_id, dm_sage.id.clone(), 2, Some("DM with Sage".to_string()))?;

    // === Intentions & Quests ===

    // Build P2P Workspace (Quest)
    let quest = vault.place_tree(TreeType::Quest, all_audience.clone(), now)?;
    let quest_id = quest.id.clone();
    vault.compose(&root_id, quest_id.clone(), 3, Some("Build P2P Workspace".to_string()))?;

    // Need: Logo Design
    let need = vault.place_tree(TreeType::Need, all_audience.clone(), now)?;
    vault.compose(&root_id, need.id.clone(), 4, Some("Need: Logo Design".to_string()))?;

    // Offering: Code Review
    let offering = vault.place_tree(TreeType::Offering, all_audience.clone(), now)?;
    vault.compose(&root_id, offering.id.clone(), 5, Some("Offering: Code Review".to_string()))?;

    // Intention: Learn Rust
    let intention = vault.place_tree(TreeType::Intention, vec![player_id], now)?;
    let intention_id = intention.id.clone();
    vault.compose(&root_id, intention_id.clone(), 6, Some("Intention: Learn Rust".to_string()))?;

    // === Quest descriptions ===

    // Build P2P Workspace description
    let quest_desc_text = "Design and implement the IndrasNetwork workspace interface \u{2014} a P2P collaborative environment built on the new artifact ontology. Needs responsive layout, all TreeType views, and working attention heat visualization.";
    let quest_desc = vault.place_leaf(
        quest_desc_text.as_bytes(),
        LeafType::Message, now,
    )?;
    vault.compose(&quest_id, quest_desc.id, 0, Some("description".to_string()))?;

    // Need: Logo Design description
    let need_desc = vault.place_leaf(
        b"Looking for a designer to create a logo for IndrasNetwork. Should evoke peer-to-peer connectivity, fractal structure, and attention flow. Vector format preferred.",
        LeafType::Message, now,
    )?;
    vault.compose(&need.id, need_desc.id, 0, Some("description".to_string()))?;

    // Offering: Code Review description
    let offering_desc = vault.place_leaf(
        b"Offering code review for Rust projects related to CRDT implementations. Experienced with Yrs, Automerge, and custom operational transform designs.",
        LeafType::Message, now,
    )?;
    vault.compose(&offering.id, offering_desc.id, 0, Some("description".to_string()))?;

    // Intention: Learn Rust description
    let intention_desc = vault.place_leaf(
        b"Personal goal to become proficient in Rust systems programming. Focus areas: async runtime internals, trait-based architecture, and WASM compilation targets.",
        LeafType::Message, now,
    )?;
    vault.compose(&intention_id, intention_desc.id, 0, Some("description".to_string()))?;

    // === Exchanges ===
    let exchange = vault.place_tree(TreeType::Exchange, vec![player_id, peers[1].player_id], now)?;
    vault.compose(&root_id, exchange.id.clone(), 7, Some("Exchange with Zephyr".to_string()))?;

    // === Tokens ===
    let tokens = vault.place_tree(TreeType::Collection, vec![player_id], now)?;
    vault.compose(&root_id, tokens.id.clone(), 8, Some("Tokens of Gratitude (3)".to_string()))?;

    // === Seed attention data for heat ===
    // Navigate through items to create attention events
    vault.navigate_to(project_alpha_id.clone(), now)?;
    vault.navigate_to(arch_notes_id.clone(), now + 1000)?;
    vault.navigate_to(team_discussion_id.clone(), now + 900_000)?;  // 15 min later
    vault.navigate_to(arch_notes_id.clone(), now + 1_800_000)?;     // 30 min later
    vault.navigate_to(quest_id.clone(), now + 2_400_000)?;          // 40 min later

    // Ingest peer attention for heat
    use indras_artifacts::AttentionSwitchEvent;
    let sage_events = vec![
        AttentionSwitchEvent {
            player: peers[0].player_id,
            from: None,
            to: Some(team_discussion_id.clone()),
            timestamp: now,
        },
        AttentionSwitchEvent {
            player: peers[0].player_id,
            from: Some(team_discussion_id.clone()),
            to: Some(arch_notes_id.clone()),
            timestamp: now + 600_000,
        },
        AttentionSwitchEvent {
            player: peers[0].player_id,
            from: Some(arch_notes_id.clone()),
            to: Some(need.id.clone()),
            timestamp: now + 1_200_000,
        },
    ];
    vault.ingest_peer_log(peers[0].player_id, sage_events)?;

    let zephyr_events = vec![
        AttentionSwitchEvent {
            player: peers[1].player_id,
            from: None,
            to: Some(team_discussion_id.clone()),
            timestamp: now + 300_000,
        },
        AttentionSwitchEvent {
            player: peers[1].player_id,
            from: Some(team_discussion_id.clone()),
            to: Some(project_alpha_id.clone()),
            timestamp: now + 1_500_000,
        },
    ];
    vault.ingest_peer_log(peers[1].player_id, zephyr_events)?;

    Ok(VaultHandle {
        vault: Arc::new(Mutex::new(vault)),
        player_id,
        player_name: "Nova".to_string(),
    })
}
