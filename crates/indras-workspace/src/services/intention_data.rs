//! Intention view data construction.
//!
//! Extracts the ~200 lines of QuestViewData construction from app.rs into
//! standalone functions. Renames QuestViewData to IntentionViewData to match
//! the domain language.

use indras_artifacts::{Intention, PlayerId, ArtifactId};
use crate::bridge::vault_bridge::InMemoryVault;
use crate::components::intention_board::IntentionCardData;
use indras_sync_engine::IntentionKind;
use crate::components::intention_view::{
    ProofEntry, AttentionItem, PledgedToken,
    AttentionPeerSummary, StewardshipChainEntry, format_duration_secs,
};

/// View data for rendering an intention (formerly QuestViewData).
///
/// Contains all the pre-computed display data needed by the QuestView
/// component, built from vault artifacts.
#[derive(Clone, Debug)]
pub struct IntentionViewData {
    /// The kind of intention (Quest, Need, Offering, Intention).
    pub kind: IntentionKind,
    /// Display title.
    pub title: String,
    /// Description text.
    pub description: String,
    /// Status string (Open, Proven, Fulfilled).
    pub status: String,
    /// Name of the steward.
    pub steward_name: String,
    /// Number of audience members.
    pub audience_count: usize,
    /// Proof entries submitted against this intention.
    pub proofs: Vec<ProofEntry>,
    /// How long ago the intention was posted.
    pub posted_ago: String,
    /// Heat score (0.0 to 1.0+).
    pub heat: f32,
    /// Per-peer attention summaries.
    pub attention_peers: Vec<AttentionPeerSummary>,
    /// Total attention duration across all peers.
    pub total_attention_duration: String,
    /// Tokens pledged to this intention.
    pub pledged_tokens: Vec<PledgedToken>,
    /// Chain of stewardship transfers.
    pub stewardship_chain: Vec<StewardshipChainEntry>,
    /// Attention items for the local player (for inline picker).
    pub attention_items: Vec<AttentionItem>,
    /// The artifact ID of this intention.
    pub intention_id: ArtifactId,
}

/// Resolve a player ID to display info (name, letter, CSS class).
pub fn peer_display_info(player_id: PlayerId, player_name: &str) -> (String, String, String) {
    if player_id == [1u8; 32] {
        (player_name.to_string(), player_name.chars().next().unwrap_or('N').to_string(), "peer-dot-self".into())
    } else if player_id == [2u8; 32] {
        ("Sage".into(), "S".into(), "peer-dot-sage".into())
    } else if player_id == [3u8; 32] {
        ("Zephyr".into(), "Z".into(), "peer-dot-zeph".into())
    } else {
        let hex: String = player_id.iter().take(4).map(|b| format!("{b:02x}")).collect();
        (hex.clone(), hex.chars().next().unwrap_or('?').to_string(), String::new())
    }
}

/// Build the complete `IntentionViewData` for a given intention artifact.
///
/// Reads proofs, status, heat, attention, pledged tokens, and stewardship
/// chain from the vault, returning a fully-populated view data struct.
pub fn build_intention_view_data(
    vault: &InMemoryVault,
    artifact_id: ArtifactId,
    artifact_type: &str,
    label: &str,
    player_id: PlayerId,
    player_name: &str,
) -> Option<IntentionViewData> {
    let artifact = vault.get_artifact(&artifact_id).ok().flatten()?;

    let kind = match artifact_type {
        "need" => IntentionKind::Need,
        "offering" => IntentionKind::Offering,
        "intention" => IntentionKind::Intention,
        _ => IntentionKind::Quest,
    };

    let steward_name = if artifact.steward == player_id {
        player_name.to_string()
    } else {
        vault.peers().iter()
            .find(|p| p.peer_id == artifact.steward)
            .and_then(|p| p.display_name.clone())
            .unwrap_or_else(|| "Unknown".to_string())
    };
    let audience_count = artifact.grants.len();

    // Build description from first ref with "description" label
    let description = {
        let desc_ref = artifact.references.iter()
            .find(|r| r.label.as_deref() == Some("description"));
        if let Some(dr) = desc_ref {
            vault.get_payload(&dr.artifact_id)
                .ok()
                .flatten()
                .map(|p| String::from_utf8_lossy(&p).to_string())
                .unwrap_or_else(|| "No description yet.".to_string())
        } else {
            "No description yet.".to_string()
        }
    };

    let intention = Intention::from_id(artifact.id);

    // Load proofs
    let proofs = build_proofs(vault, &intention, player_name);

    // Status
    let status_str = match intention.status(vault) {
        Ok(Some(s)) if s == "fulfilled" => "Fulfilled",
        _ => if proofs.is_empty() { "Open" } else { "Proven" },
    };

    // Heat
    let now_ms = chrono::Utc::now().timestamp_millis();
    let heat = vault.heat(&artifact.id, now_ms).unwrap_or(0.0);

    // Attention per peer
    let attention_peers = build_attention_peers(vault, &intention, &artifact, player_name);

    let total_attention_duration = {
        let total: u64 = attention_peers.iter().map(|p| p.total_duration_secs).sum();
        format_duration_secs(total)
    };

    // Attention items for local player (for inline picker)
    let attention_items = build_attention_items(vault, &intention, player_id);

    // Pledged tokens
    let pledged_tokens = build_pledged_tokens(vault, &intention, player_name);

    // Stewardship chain
    let stewardship_chain = build_stewardship_chain(vault, &intention, player_name);

    Some(IntentionViewData {
        kind,
        title: label.to_string(),
        description,
        status: status_str.to_string(),
        steward_name,
        audience_count,
        proofs,
        posted_ago: String::new(),
        heat,
        attention_peers,
        total_attention_duration,
        pledged_tokens,
        stewardship_chain,
        attention_items,
        intention_id: artifact.id,
    })
}

/// Build lightweight card summaries for all intention-type artifacts in the vault.
///
/// Scans the root artifact's children for quest/need/offering/intention types
/// and returns card data sorted by heat (descending).
pub fn build_intention_cards(
    vault: &InMemoryVault,
    player_id: PlayerId,
    _player_name: &str,
) -> Vec<IntentionCardData> {
    let root_id = vault.root.id.clone();
    let root = match vault.get_artifact(&root_id) {
        Ok(Some(a)) => a,
        _ => return Vec::new(),
    };

    let mut cards = Vec::new();
    for child_ref in &root.references {
        if let Ok(Some(artifact)) = vault.get_artifact(&child_ref.artifact_id) {
            let art_type = &artifact.artifact_type;
            if !matches!(art_type.as_str(), "quest" | "need" | "offering" | "intention") {
                continue;
            }

            let kind = match art_type.as_str() {
                "need" => IntentionKind::Need,
                "offering" => IntentionKind::Offering,
                "intention" => IntentionKind::Intention,
                _ => IntentionKind::Quest,
            };

            let title = child_ref.label.as_deref().unwrap_or("Untitled").to_string();
            let intention = Intention::from_id(artifact.id);

            let description = artifact.references.iter()
                .find(|r| r.label.as_deref() == Some("description"))
                .and_then(|r| vault.get_payload(&r.artifact_id).ok().flatten())
                .map(|p| String::from_utf8_lossy(&p).to_string())
                .unwrap_or_default();

            let proof_count = intention.proofs(vault).map(|p| p.len()).unwrap_or(0);
            let token_count = intention.pledged_tokens(vault).map(|t| t.len()).unwrap_or(0);

            let now_ms = chrono::Utc::now().timestamp_millis();
            let heat = vault.heat(&artifact.id, now_ms).unwrap_or(0.0);

            let is_complete = intention.status(vault)
                .ok()
                .flatten()
                .map(|s| s == "fulfilled")
                .unwrap_or(false);

            let attention_secs: u64 = intention
                .unreleased_attention(vault, player_id)
                .unwrap_or_default()
                .iter()
                .map(|w| w.duration_ms / 1000)
                .sum();
            let attention_duration = if attention_secs > 0 {
                format_duration_secs(attention_secs)
            } else {
                String::new()
            };

            cards.push(IntentionCardData {
                id: format!("{:?}", artifact.id),
                kind,
                title,
                description,
                proof_count,
                token_count,
                attention_duration,
                heat,
                is_complete,
            });
        }
    }

    cards.sort_by(|a, b| b.heat.partial_cmp(&a.heat).unwrap_or(std::cmp::Ordering::Equal));
    cards
}

/// Build proof entries from an intention's proof references.
fn build_proofs(
    vault: &InMemoryVault,
    intention: &Intention,
    player_name: &str,
) -> Vec<ProofEntry> {
    let proof_refs = intention.proofs(vault).unwrap_or_default();
    let mut proof_entries = Vec::new();
    for proof_ref in &proof_refs {
        let (author_name, author_letter, author_color) = if let Some(label) = &proof_ref.label {
            let parts: Vec<&str> = label.splitn(3, ':').collect();
            if parts.len() >= 2 {
                let hex = parts[1];
                if hex.starts_with("02") {
                    ("Sage".into(), "S".into(), "peer-dot-sage".into())
                } else if hex.starts_with("03") {
                    ("Zephyr".into(), "Z".into(), "peer-dot-zeph".into())
                } else {
                    (player_name.to_string(), player_name.chars().next().unwrap_or('N').to_string(), "peer-dot-self".into())
                }
            } else {
                ("Unknown".into(), "?".into(), String::new())
            }
        } else {
            ("Unknown".into(), "?".into(), String::new())
        };

        let body = vault.get_payload(&proof_ref.artifact_id)
            .ok()
            .flatten()
            .map(|p| String::from_utf8_lossy(&p).to_string())
            .unwrap_or_else(|| "Proof submitted".to_string());

        proof_entries.push(ProofEntry {
            author_name,
            author_letter,
            author_color_class: author_color,
            body,
            time_ago: "recently".into(),
            artifact_attachments: Vec::new(),
            tokens: Vec::new(),
            has_tokens: false,
            total_token_count: 0,
            total_token_duration: String::new(),
        });
    }
    proof_entries
}

/// Build per-peer attention summaries for an intention.
fn build_attention_peers(
    vault: &InMemoryVault,
    intention: &Intention,
    artifact: &indras_artifacts::Artifact,
    player_name: &str,
) -> Vec<AttentionPeerSummary> {
    let audience_ids: Vec<PlayerId> = artifact.grants.iter().map(|g| g.grantee).collect();
    let mut max_secs = 0u64;

    // First pass: compute totals
    let mut peer_data: Vec<(PlayerId, Vec<indras_artifacts::DwellWindow>)> = Vec::new();
    for &pid in &audience_ids {
        let windows = intention.unreleased_attention(vault, pid).unwrap_or_default();
        if !windows.is_empty() {
            let total_ms: u64 = windows.iter().map(|w| w.duration_ms).sum();
            let total_secs = total_ms / 1000;
            if total_secs > max_secs { max_secs = total_secs; }
            peer_data.push((pid, windows));
        }
    }

    // Second pass: build summaries with bar fractions
    let mut peers_summary = Vec::new();
    for (pid, windows) in &peer_data {
        let total_ms: u64 = windows.iter().map(|w| w.duration_ms).sum();
        let total_secs = total_ms / 1000;
        let (name, letter, color) = peer_display_info(*pid, player_name);
        peers_summary.push(AttentionPeerSummary {
            peer_name: name,
            peer_letter: letter,
            peer_color_class: color,
            total_duration: format_duration_secs(total_secs),
            total_duration_secs: total_secs,
            window_count: windows.len(),
            bar_fraction: if max_secs > 0 { total_secs as f32 / max_secs as f32 } else { 0.0 },
        });
    }
    peers_summary
}

/// Build attention items for the local player (used in inline picker).
fn build_attention_items(
    vault: &InMemoryVault,
    intention: &Intention,
    player_id: PlayerId,
) -> Vec<AttentionItem> {
    let windows = intention.unreleased_attention(vault, player_id).unwrap_or_default();
    windows.iter().map(|w| {
        AttentionItem {
            target: "This Intention".into(),
            when: format!("{}ms ago", w.start_timestamp),
            duration: format_duration_secs(w.duration_ms / 1000),
        }
    }).collect()
}

/// Build pledged token entries for an intention.
fn build_pledged_tokens(
    vault: &InMemoryVault,
    intention: &Intention,
    player_name: &str,
) -> Vec<PledgedToken> {
    let pledge_refs = intention.pledged_tokens(vault).unwrap_or_default();
    let mut pts = Vec::new();
    for pref in &pledge_refs {
        let duration = vault.get_payload(&pref.artifact_id)
            .ok()
            .flatten()
            .and_then(|p| {
                if p.len() >= 8 {
                    Some(u64::from_le_bytes(p[..8].try_into().unwrap_or([0u8; 8])))
                } else {
                    None
                }
            })
            .unwrap_or(0);
        let from_name = if let Some(label) = &pref.label {
            let parts: Vec<&str> = label.splitn(3, ':').collect();
            if parts.len() >= 2 {
                let hex = parts[1];
                if hex.starts_with("02") { "Sage".into() }
                else if hex.starts_with("03") { "Zephyr".into() }
                else { player_name.to_string() }
            } else {
                "Unknown".into()
            }
        } else {
            "Unknown".into()
        };
        pts.push(PledgedToken {
            token_label: "Token".to_string(),
            duration: format_duration_secs(duration / 1000),
            from_name,
        });
    }
    pts
}

/// Build the stewardship chain for an intention.
fn build_stewardship_chain(
    vault: &InMemoryVault,
    intention: &Intention,
    player_name: &str,
) -> Vec<StewardshipChainEntry> {
    let mut chain = Vec::new();

    // From proof refs -- each proof represents a "created" link
    let proof_refs = intention.proofs(vault).unwrap_or_default();
    for pref in &proof_refs {
        if let Ok(Some(artifact)) = vault.get_artifact(&pref.artifact_id) {
            let (from_name, from_letter, from_color) = peer_display_info(artifact.steward, player_name);
            for blessing in &artifact.blessing_history {
                let (bn, bl, bc) = peer_display_info(blessing.from, player_name);
                chain.push(StewardshipChainEntry {
                    from_name: bn,
                    from_letter: bl,
                    from_color_class: bc,
                    action: "blessed".into(),
                    token_label: "Token".into(),
                    token_duration: String::new(),
                    to_name: from_name.clone(),
                    to_letter: from_letter.clone(),
                    to_color_class: from_color.clone(),
                });
            }
        }
    }

    // From steward_history on any known tokens
    let pledge_refs = intention.pledged_tokens(vault).unwrap_or_default();
    for pref in &pledge_refs {
        if let Ok(history) = vault.steward_history(&pref.artifact_id) {
            for record in &history {
                let (fn_, fl, fc) = peer_display_info(record.from, player_name);
                let (tn, tl, tc) = peer_display_info(record.to, player_name);
                chain.push(StewardshipChainEntry {
                    from_name: fn_,
                    from_letter: fl,
                    from_color_class: fc,
                    action: "released".into(),
                    token_label: "Token".into(),
                    token_duration: String::new(),
                    to_name: tn,
                    to_letter: tl,
                    to_color_class: tc,
                });
            }
        }
    }

    chain
}
