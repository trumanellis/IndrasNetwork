//! Realm-backed intention data construction.
//!
//! Reads from CRDT documents in the home realm, replacing the
//! vault-based `intention_data.rs`. Uses `HomeRealmIntentions`
//! for the intention document and direct document access for
//! attention, blessings, and tokens.

use std::collections::HashMap;

use indras_network::home_realm::HomeRealm;
use indras_network::member::MemberId;
use indras_network::IndrasNetwork;
use indras_sync_engine::{
    AttentionDocument, BlessingDocument, ClaimId, IntentionDocument, IntentionId, IntentionKind,
    HomeRealmIntentions, TokenOfGratitude, TokenOfGratitudeDocument,
};

use crate::components::intention_board::IntentionCardData;
use crate::components::intention_view::{
    AttentionItem, AttentionPeerSummary, PledgedToken, ProofEntry, StewardshipChainEntry,
    format_duration_secs,
};

/// View data for rendering an intention from CRDT documents.
#[derive(Clone, Debug)]
pub struct IntentionViewData {
    /// The kind of intention (Quest, Need, Offering, Intention).
    pub kind: IntentionKind,
    /// Display title.
    pub title: String,
    /// Description text.
    pub description: String,
    /// Status string (Open, Proven, Verified, Fulfilled).
    pub status: String,
    /// Name of the creator/steward.
    pub steward_name: String,
    /// Number of realm members.
    pub audience_count: usize,
    /// Proof entries (service claims).
    pub proofs: Vec<ProofEntry>,
    /// How long ago the intention was posted.
    pub posted_ago: String,
    /// Heat score (0.0 to 1.0).
    pub heat: f32,
    /// Per-peer attention summaries.
    pub attention_peers: Vec<AttentionPeerSummary>,
    /// Total attention duration across all peers.
    pub total_attention_duration: String,
    /// Tokens pledged to this intention.
    pub pledged_tokens: Vec<PledgedToken>,
    /// Chain of stewardship transfers.
    pub stewardship_chain: Vec<StewardshipChainEntry>,
    /// Unblessed attention events for the local member.
    pub attention_items: Vec<AttentionItem>,
    /// The CRDT intention ID.
    pub intention_id: IntentionId,
    /// The creator's MemberId.
    pub creator: MemberId,
    /// Members currently focused on this intention.
    pub currently_focused: Vec<crate::components::intention_view::FocusedMember>,
}

/// Summary card for a token in the wallet view.
#[derive(Clone, Debug, PartialEq)]
pub struct TokenCardData {
    /// Hex-encoded token ID.
    pub id: String,
    /// Source description (e.g. "from A on 'Help garden'").
    pub blessing_source: String,
    /// Attention duration backing this token.
    pub attention_duration: String,
    /// Intention title if pledged.
    pub pledged_to: Option<String>,
    /// Length of the steward chain.
    pub steward_chain_len: usize,
    /// Number of attention events backing this token.
    pub attention_event_count: usize,
    /// Display name of the blesser.
    pub blesser_name: String,
    /// First letter of the blesser's name.
    pub blesser_letter: String,
    /// CSS color class for the blesser dot.
    pub blesser_color_class: String,
    /// Title of the source intention.
    pub source_intention_title: String,
    /// Display name of the current holder (steward).
    pub current_holder_name: String,
    /// First letter of the current holder's name.
    pub current_holder_letter: String,
    /// CSS color class for the current holder dot.
    pub current_holder_color_class: String,
    /// Human-readable "time ago" since token was created.
    pub created_ago: String,
    /// Ordered list of steward chain dots for visualization.
    pub steward_chain: Vec<StewardChainDot>,
}

/// A single dot in the steward chain visualization.
#[derive(Clone, Debug, PartialEq)]
pub struct StewardChainDot {
    pub letter: String,
    pub color_class: String,
    pub name: String,
}

// ================================================================
// Helpers
// ================================================================

const PEER_COLORS: &[&str] = &["peer-dot-sage", "peer-dot-zeph", "peer-dot-rose"];

/// Resolve a MemberId to display info (name, letter, CSS class).
fn member_display(
    id: &MemberId,
    local_id: &MemberId,
    local_name: &str,
    index: usize,
) -> (String, String, String) {
    if id == local_id {
        let letter = local_name.chars().next().unwrap_or('Y').to_string();
        return (local_name.to_string(), letter, "peer-dot-self".to_string());
    }
    let name: String = id.iter().take(4).map(|b| format!("{b:02x}")).collect();
    let letter = name.chars().next().unwrap_or('?').to_string();
    let color = PEER_COLORS[index % PEER_COLORS.len()].to_string();
    (name, letter, color)
}

/// Format a timestamp as "Xm ago" / "Xh ago" / "Xd ago".
fn time_ago(timestamp_millis: i64) -> String {
    let now_ms = chrono::Utc::now().timestamp_millis();
    let elapsed_ms = (now_ms - timestamp_millis).max(0);
    if elapsed_ms < 60_000 {
        "just now".to_string()
    } else if elapsed_ms < 3_600_000 {
        format!("{}m ago", elapsed_ms / 60_000)
    } else if elapsed_ms < 86_400_000 {
        format!("{}h ago", elapsed_ms / 3_600_000)
    } else {
        format!("{}d ago", elapsed_ms / 86_400_000)
    }
}

// ================================================================
// Public API
// ================================================================

/// Build lightweight card summaries for all intentions in the home realm.
///
/// Reads from the CRDT IntentionDocument, computing attention heat
/// and token counts via direct document access.
pub async fn build_intention_cards(
    home: &HomeRealm,
    _member_id: MemberId,
) -> Vec<IntentionCardData> {
    // Collect intention data while holding the read lock, then drop it.
    let intention_data: Vec<_> = {
        let doc = match home.intentions().await {
            Ok(d) => d,
            Err(e) => {
                tracing::warn!(error = %e, "Failed to read intentions document");
                return Vec::new();
            }
        };
        let data = doc.read().await;
        data.intentions
            .iter()
            .filter(|i| !i.deleted)
            .map(|i| {
                (
                    i.id,
                    i.kind.clone(),
                    i.title.clone(),
                    i.description.clone(),
                    i.claims.len(),
                    i.is_complete(),
                )
            })
            .collect()
    };

    // Read attention and token documents once for the whole loop.
    let attention_doc = home.document::<AttentionDocument>("attention").await.ok();
    let token_doc = home.document::<TokenOfGratitudeDocument>("_tokens").await.ok();

    let mut cards = Vec::new();
    for (id, kind, title, desc, claim_count, complete) in intention_data {
        let total_ms = if let Some(ref adoc) = attention_doc {
            let data = adoc.read().await;
            data.intention_attention(&id, None).total_attention_millis
        } else {
            0
        };

        let token_count = if let Some(ref tdoc) = token_doc {
            let data = tdoc.read().await;
            data.pledged_tokens_for_intention(&id).len()
        } else {
            0
        };

        // Heat: normalize attention to 0..1 range (30 min = 1.0)
        let heat = (total_ms as f32 / 1_800_000.0).min(1.0);
        let attention_secs = total_ms / 1000;
        let attention_duration = if attention_secs > 0 {
            format_duration_secs(attention_secs)
        } else {
            String::new()
        };

        let id_hex: String = id.iter().map(|b| format!("{b:02x}")).collect();

        cards.push(IntentionCardData {
            id: id_hex,
            kind,
            title,
            description: desc,
            proof_count: claim_count,
            token_count,
            attention_duration,
            heat,
            is_complete: complete,
        });
    }

    cards.sort_by(|a, b| {
        b.heat
            .partial_cmp(&a.heat)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    cards
}

/// Build the complete view data for a single intention.
///
/// Reads the intention, its claims, attention data, blessings, and
/// pledged tokens from the home realm's CRDT documents.
pub async fn build_intention_view(
    home: &HomeRealm,
    intention_id: IntentionId,
    member_id: MemberId,
    local_name: &str,
) -> Option<IntentionViewData> {
    // Clone intention data before dropping the document guard.
    let intention = {
        let doc = home.intentions().await.ok()?;
        let data = doc.read().await;
        data.intentions
            .iter()
            .find(|i| i.id == intention_id && !i.deleted)?
            .clone()
    };

    let creator_name = if intention.creator == member_id {
        local_name.to_string()
    } else {
        intention
            .creator
            .iter()
            .take(4)
            .map(|b| format!("{b:02x}"))
            .collect()
    };

    // Status
    let status = if intention.is_complete() {
        "Fulfilled"
    } else if intention.has_verified_claims() {
        "Verified"
    } else if intention.has_claims() {
        "Proven"
    } else {
        "Open"
    }
    .to_string();

    // Build proof entries from service claims
    let blessing_doc = home.document::<BlessingDocument>("blessings").await.ok();
    let mut proofs = Vec::new();
    for (idx, claim) in intention.claims.iter().enumerate() {
        let (name, letter, color) = member_display(&claim.claimant, &member_id, local_name, idx);

        let token_count = if let Some(ref bdoc) = blessing_doc {
            let data = bdoc.read().await;
            let claim_id = ClaimId::new(intention_id, claim.claimant);
            data.blessings_for_claim(&claim_id).len()
        } else {
            0
        };

        let body = if claim.verified {
            "Service claim (verified)".to_string()
        } else {
            "Service claim submitted".to_string()
        };

        proofs.push(ProofEntry {
            author_name: name,
            author_letter: letter,
            author_color_class: color,
            body,
            time_ago: time_ago(claim.submitted_at_millis),
            artifact_attachments: Vec::new(),
            tokens: Vec::new(),
            has_tokens: token_count > 0,
            total_token_count: token_count,
            total_token_duration: String::new(),
            has_proof_artifact: claim.proof.is_some(),
            has_proof_folder: claim.proof_folder.is_some(),
            blessings: Vec::new(),
            total_blessed_duration: String::new(),
            is_verified: claim.verified,
            verified_ago: if claim.verified { Some("verified".to_string()) } else { None },
        });
    }

    // Attention data
    let attention_doc = home.document::<AttentionDocument>("attention").await.ok();
    let (attention_peers, total_attention_duration, heat) = if let Some(ref adoc) = attention_doc {
        let data = adoc.read().await;
        let attn = data.intention_attention(&intention_id, None);
        let total_ms = attn.total_attention_millis;
        let heat = (total_ms as f32 / 1_800_000.0).min(1.0);
        let total_dur = format_duration_secs(total_ms / 1000);

        let max_ms = attn.attention_by_member.values().copied().max().unwrap_or(0);
        let mut peer_entries: Vec<_> = attn.attention_by_member.iter().collect();
        peer_entries.sort_by(|(_, a), (_, b)| b.cmp(a));

        let mut peers = Vec::new();
        for (idx, pair) in peer_entries.iter().enumerate() {
            let mid = pair.0;
            let ms = *pair.1;
            let (name, letter, color) = member_display(mid, &member_id, local_name, idx);
            let secs = ms / 1000;
            peers.push(AttentionPeerSummary {
                peer_name: name,
                peer_letter: letter,
                peer_color_class: color,
                total_duration: format_duration_secs(secs),
                total_duration_secs: secs,
                window_count: 1,
                bar_fraction: if max_ms > 0 {
                    ms as f32 / max_ms as f32
                } else {
                    0.0
                },
            });
        }
        (peers, total_dur, heat)
    } else {
        (Vec::new(), "0m 00s".to_string(), 0.0)
    };

    // Unblessed attention events for the local member (for token picker)
    let attention_items: Vec<AttentionItem> =
        if let (Some(adoc), Some(bdoc)) = (&attention_doc, &blessing_doc) {
            let adata = adoc.read().await;
            let bdata = bdoc.read().await;
            let events = adata.events();
            let candidate_indices: Vec<usize> = events
                .iter()
                .enumerate()
                .filter(|(_, e)| e.member == member_id && e.intention_id == Some(intention_id))
                .map(|(idx, _)| idx)
                .collect();
            bdata
                .unblessed_event_indices(&member_id, &intention_id, &candidate_indices)
                .iter()
                .map(|&idx| AttentionItem {
                    target: "This Intention".into(),
                    when: format!("Event #{}", idx),
                    duration: "\u{2014}".into(),
                })
                .collect()
        } else {
            Vec::new()
        };

    // Pledged tokens
    let token_doc = home
        .document::<TokenOfGratitudeDocument>("_tokens")
        .await
        .ok();
    let pledged_tokens_data: Vec<TokenOfGratitude> = if let Some(ref tdoc) = token_doc {
        let data = tdoc.read().await;
        data.pledged_tokens_for_intention(&intention_id)
            .into_iter()
            .cloned()
            .collect()
    } else {
        Vec::new()
    };

    let pledged_tokens: Vec<PledgedToken> = pledged_tokens_data
        .iter()
        .enumerate()
        .map(|(idx, t)| {
            let (name, _, _) = member_display(&t.steward, &member_id, local_name, idx);
            PledgedToken {
                token_label: format!("Token #{}", idx + 1),
                duration: String::new(),
                from_name: name,
            }
        })
        .collect();

    // Stewardship chain from token transfer history
    let mut stewardship_chain = Vec::new();
    for token in &pledged_tokens_data {
        for i in 0..token.steward_chain.len().saturating_sub(1) {
            let from = &token.steward_chain[i];
            let to = &token.steward_chain[i + 1];
            let (fn_, fl, fc) = member_display(from, &member_id, local_name, i);
            let (tn, tl, tc) = member_display(to, &member_id, local_name, i + 1);
            stewardship_chain.push(StewardshipChainEntry {
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

    Some(IntentionViewData {
        kind: intention.kind.clone(),
        title: intention.title.clone(),
        description: intention.description.clone(),
        status,
        steward_name: creator_name,
        audience_count: 1, // Home realm has one member
        proofs,
        posted_ago: time_ago(intention.created_at_millis),
        heat,
        attention_peers,
        total_attention_duration,
        pledged_tokens,
        stewardship_chain,
        attention_items,
        intention_id,
        creator: intention.creator,
        currently_focused: Vec::new(),
    })
}

/// Build token cards for the local member's wallet view.
pub async fn build_member_tokens(
    home: &HomeRealm,
    member_id: MemberId,
    local_name: &str,
) -> Vec<TokenCardData> {
    let token_doc = match home.document::<TokenOfGratitudeDocument>("_tokens").await {
        Ok(d) => d,
        Err(_) => return Vec::new(),
    };

    let tokens: Vec<TokenOfGratitude> = {
        let data = token_doc.read().await;
        data.tokens_for_steward(&member_id)
            .into_iter()
            .cloned()
            .collect()
    };

    // Build intention title lookup for pledge display
    let intention_titles: HashMap<IntentionId, String> = {
        let mut map = HashMap::new();
        if let Ok(doc) = home.intentions().await {
            let data = doc.read().await;
            for i in &data.intentions {
                map.insert(i.id, i.title.clone());
            }
        }
        map
    };

    tokens
        .iter()
        .enumerate()
        .map(|(idx, t)| {
            let id_hex: String = t.id.iter().map(|b| format!("{b:02x}")).collect();
            let source_title = intention_titles
                .get(&t.source_intention_id)
                .cloned()
                .unwrap_or_else(|| "Unknown intention".to_string());
            let pledged_to = t
                .pledged_to
                .as_ref()
                .and_then(|iid| intention_titles.get(iid).cloned());

            // Blesser display
            let (blesser_name, blesser_letter, blesser_color_class) =
                member_display(&t.blesser, &member_id, local_name, idx);

            // Current holder (steward)
            let (current_holder_name, current_holder_letter, current_holder_color_class) =
                member_display(&t.steward, &member_id, local_name, idx + 1);

            // Build steward chain dots
            let steward_chain: Vec<StewardChainDot> = t.steward_chain
                .iter()
                .enumerate()
                .map(|(i, mid)| {
                    let (name, letter, color) = member_display(mid, &member_id, local_name, i);
                    StewardChainDot { letter, color_class: color, name }
                })
                .collect();

            TokenCardData {
                id: id_hex,
                blessing_source: format!("from {} on '{}'", blesser_name, source_title),
                attention_duration: "\u{2014}".to_string(),
                pledged_to,
                steward_chain_len: t.steward_chain.len(),
                attention_event_count: 0,
                blesser_name,
                blesser_letter,
                blesser_color_class,
                source_intention_title: source_title,
                current_holder_name,
                current_holder_letter,
                current_holder_color_class,
                created_ago: time_ago(t.created_at_millis),
                steward_chain,
            }
        })
        .collect()
}

/// Build intention cards from all DM realms (community/shared intentions).
///
/// Iterates conversation realms, filters to DM realms, reads each realm's
/// `IntentionDocument`, and returns cards for intentions not created by
/// the local member.
pub async fn build_community_intention_cards(
    network: &IndrasNetwork,
    my_member_id: MemberId,
) -> Vec<IntentionCardData> {
    let mut cards = Vec::new();
    let realm_ids = network.conversation_realms();

    for realm_id in realm_ids {
        // Only process DM realms
        if network.dm_peer_for_realm(&realm_id).is_none() {
            continue;
        }

        let realm = match network.get_realm_by_id(&realm_id) {
            Some(r) => r,
            None => continue,
        };

        let doc = match realm.document::<IntentionDocument>("intentions").await {
            Ok(d) => d,
            Err(_) => continue,
        };

        let data = doc.read().await;
        for intention in &data.intentions {
            if intention.deleted {
                continue;
            }
            // Skip our own intentions (we see them in My Intentions)
            if intention.creator == my_member_id {
                continue;
            }

            let id_hex: String = intention.id.iter().map(|b| format!("{b:02x}")).collect();
            cards.push(IntentionCardData {
                id: id_hex,
                kind: intention.kind.clone(),
                title: intention.title.clone(),
                description: intention.description.clone(),
                proof_count: intention.claims.len(),
                token_count: 0,
                attention_duration: String::new(),
                heat: 0.0,
                is_complete: intention.is_complete(),
            });
        }
    }

    cards
}
