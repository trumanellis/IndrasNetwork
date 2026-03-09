//! View data builders — read from CRDT documents and produce structs for rendering.
//!
//! Follows the same pattern as `indras-workspace/src/services/realm_data.rs`:
//! open document, read-lock, extract plain data, drop lock, build view structs.

use std::collections::HashMap;

use indras_network::home_realm::HomeRealm;
use indras_network::member::MemberId;
use indras_network::IndrasNetwork;
use indras_sync_engine::{
    AttentionDocument, BlessingDocument, ClaimId, IntentionDocument, IntentionId, IntentionKind,
    HomeRealmIntentions, TokenOfGratitude, TokenOfGratitudeDocument,
};

// ================================================================
// View data types
// ================================================================

/// Lightweight card for the intention feed.
#[derive(Clone, Debug, PartialEq)]
pub struct IntentionCardData {
    /// Hex-encoded IntentionId.
    pub id: String,
    /// Raw IntentionId bytes.
    pub raw_id: IntentionId,
    /// Kind of intention.
    pub kind: IntentionKind,
    /// Display title.
    pub title: String,
    /// Description preview.
    pub description: String,
    /// Number of service claims.
    pub proof_count: usize,
    /// Number of tokens pledged.
    pub token_count: usize,
    /// Formatted attention duration.
    pub attention_duration: String,
    /// Heat score (0.0 to 1.0).
    pub heat: f32,
    /// Whether the intention is completed.
    pub is_complete: bool,
    /// Creator display info.
    pub creator_name: String,
    /// Creator letter avatar.
    pub creator_letter: String,
    /// Creator color class.
    pub creator_color_class: String,
    /// Status string.
    pub status: String,
    /// Time since creation.
    pub posted_ago: String,
}

/// Full detail view data for a single intention.
#[derive(Clone, Debug, PartialEq)]
pub struct IntentionViewData {
    /// The kind of intention.
    pub kind: IntentionKind,
    /// Display title.
    pub title: String,
    /// Description text.
    pub description: String,
    /// Status string (Open, Proven, Verified, Fulfilled).
    pub status: String,
    /// Name of the creator.
    pub creator_name: String,
    /// Creator letter avatar.
    pub creator_letter: String,
    /// Creator color class.
    pub creator_color_class: String,
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
    pub pledged_tokens: Vec<PledgedTokenData>,
    /// Unblessed attention events for the local member.
    pub unblessed_event_indices: Vec<usize>,
    /// The CRDT intention ID.
    pub intention_id: IntentionId,
    /// The creator's MemberId.
    pub creator: MemberId,
}

/// A proof of service entry.
#[derive(Clone, Debug, PartialEq)]
pub struct ProofEntry {
    /// Claimant display name.
    pub author_name: String,
    /// Claimant letter avatar.
    pub author_letter: String,
    /// Claimant color class.
    pub author_color_class: String,
    /// Description of the claim.
    pub body: String,
    /// When the claim was submitted.
    pub time_ago: String,
    /// Whether the claim has been verified.
    pub is_verified: bool,
    /// The claimant's MemberId.
    pub claimant: MemberId,
    /// Number of blessings on this claim.
    pub blessing_count: usize,
}

/// Per-peer attention summary.
#[derive(Clone, Debug, PartialEq)]
pub struct AttentionPeerSummary {
    /// Peer display name.
    pub peer_name: String,
    /// Peer letter avatar.
    pub peer_letter: String,
    /// Peer color class.
    pub peer_color_class: String,
    /// Formatted duration.
    pub total_duration: String,
    /// Duration in seconds.
    pub total_duration_secs: u64,
    /// Fraction of max peer's attention (for bar visualization).
    pub bar_fraction: f32,
}

/// A token pledged to an intention.
#[derive(Clone, Debug, PartialEq)]
pub struct PledgedTokenData {
    /// Token label.
    pub token_label: String,
    /// Formatted duration backing this token.
    pub duration: String,
    /// Name of the steward who pledged.
    pub from_name: String,
}

/// Summary card for a token in the wallet view.
#[derive(Clone, Debug, PartialEq)]
pub struct TokenCardData {
    /// Hex-encoded token ID.
    pub id: String,
    /// Raw token ID bytes.
    pub raw_id: [u8; 16],
    /// Source description.
    pub blessing_source: String,
    /// Attention duration backing this token.
    pub attention_duration: String,
    /// Intention title if pledged.
    pub pledged_to: Option<String>,
    /// Number of stewards in the chain.
    pub steward_chain_len: usize,
    /// Display name of the blesser.
    pub blesser_name: String,
    /// First letter of the blesser's name.
    pub blesser_letter: String,
    /// CSS color class for the blesser dot.
    pub blesser_color_class: String,
    /// Title of the source intention.
    pub source_intention_title: String,
    /// Display name of the current holder.
    pub current_holder_name: String,
    /// First letter of the current holder's name.
    pub current_holder_letter: String,
    /// CSS color class for the current holder dot.
    pub current_holder_color_class: String,
    /// Time since token was created.
    pub created_ago: String,
    /// Steward chain dots.
    pub steward_chain: Vec<StewardChainDot>,
}

/// A single dot in the steward chain visualization.
#[derive(Clone, Debug, PartialEq)]
pub struct StewardChainDot {
    /// Letter avatar.
    pub letter: String,
    /// CSS color class.
    pub color_class: String,
    /// Display name.
    pub name: String,
}

/// Display info for a connected peer dot in the PeerBar.
#[derive(Clone, Debug, PartialEq)]
pub struct PeerDisplayInfo {
    /// Peer display name.
    pub name: String,
    /// First letter for avatar dot.
    pub letter: String,
    /// CSS color class (e.g. "peer-dot-sage").
    pub color_class: String,
    /// Whether the peer is currently online.
    pub online: bool,
    /// The peer's MemberId for proactive reconnection.
    pub member_id: MemberId,
}

/// A single entry in the P2P event log footer.
#[derive(Clone, Debug, PartialEq)]
pub struct P2pLogEntry {
    /// Epoch millis timestamp.
    pub timestamp: u64,
    /// Human-readable event message.
    pub message: String,
}

// ================================================================
// Helpers
// ================================================================

/// Peer dot color classes.
pub const PEER_COLORS: &[&str] = &["peer-dot-sage", "peer-dot-zeph", "peer-dot-rose"];

/// Resolve a MemberId to (name, letter, CSS class).
pub fn member_display(
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
pub fn time_ago(timestamp_millis: i64) -> String {
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

/// Format seconds as "Xm XXs".
pub fn format_duration_secs(secs: u64) -> String {
    let m = secs / 60;
    let s = secs % 60;
    format!("{m}m {s:02}s")
}

// ================================================================
// Public API
// ================================================================

/// Build lightweight card summaries for all intentions in the home realm.
pub async fn build_intention_cards(
    home: &HomeRealm,
    member_id: MemberId,
    local_name: &str,
) -> Vec<IntentionCardData> {
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
                    i.creator,
                    i.created_at_millis,
                    i.has_verified_claims(),
                    i.has_claims(),
                )
            })
            .collect()
    };

    let attention_doc = home.document::<AttentionDocument>("attention").await.ok();
    let token_doc = home
        .document::<TokenOfGratitudeDocument>("_tokens")
        .await
        .ok();

    let mut cards = Vec::new();
    for (idx, (id, kind, title, desc, claim_count, complete, creator, created_at, has_verified, has_claims)) in
        intention_data.into_iter().enumerate()
    {
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

        let heat = (total_ms as f32 / 1_800_000.0).min(1.0);
        let attention_secs = total_ms / 1000;
        let attention_duration = if attention_secs > 0 {
            format_duration_secs(attention_secs)
        } else {
            String::new()
        };

        let id_hex: String = id.iter().map(|b| format!("{b:02x}")).collect();
        let (creator_name, creator_letter, creator_color_class) =
            member_display(&creator, &member_id, local_name, idx);

        let status = if complete {
            "Fulfilled"
        } else if has_verified {
            "Verified"
        } else if has_claims {
            "Proven"
        } else {
            "Open"
        }
        .to_string();

        cards.push(IntentionCardData {
            id: id_hex,
            raw_id: id,
            kind,
            title,
            description: desc,
            proof_count: claim_count,
            token_count,
            attention_duration,
            heat,
            is_complete: complete,
            creator_name,
            creator_letter,
            creator_color_class,
            status,
            posted_ago: time_ago(created_at),
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
pub async fn build_intention_view(
    home: &HomeRealm,
    intention_id: IntentionId,
    member_id: MemberId,
    local_name: &str,
) -> Option<IntentionViewData> {
    let intention = {
        let doc = home.intentions().await.ok()?;
        let data = doc.read().await;
        data.intentions
            .iter()
            .find(|i| i.id == intention_id && !i.deleted)?
            .clone()
    };

    let (creator_name, creator_letter, creator_color_class) =
        member_display(&intention.creator, &member_id, local_name, 0);

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

    // Build proof entries
    let blessing_doc = home.document::<BlessingDocument>("blessings").await.ok();
    let mut proofs = Vec::new();
    for (idx, claim) in intention.claims.iter().enumerate() {
        let (name, letter, color) = member_display(&claim.claimant, &member_id, local_name, idx);
        let blessing_count = if let Some(ref bdoc) = blessing_doc {
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
            is_verified: claim.verified,
            claimant: claim.claimant,
            blessing_count,
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

    // Unblessed attention events for the local member
    let unblessed_event_indices: Vec<usize> =
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
            bdata.unblessed_event_indices(&member_id, &intention_id, &candidate_indices)
        } else {
            Vec::new()
        };

    // Pledged tokens
    let token_doc = home
        .document::<TokenOfGratitudeDocument>("_tokens")
        .await
        .ok();
    let pledged_tokens: Vec<PledgedTokenData> = if let Some(ref tdoc) = token_doc {
        let data = tdoc.read().await;
        data.pledged_tokens_for_intention(&intention_id)
            .iter()
            .enumerate()
            .map(|(idx, t)| {
                let (name, _, _) = member_display(&t.steward, &member_id, local_name, idx);
                PledgedTokenData {
                    token_label: format!("Token #{}", idx + 1),
                    duration: String::new(),
                    from_name: name,
                }
            })
            .collect()
    } else {
        Vec::new()
    };

    Some(IntentionViewData {
        kind: intention.kind.clone(),
        title: intention.title.clone(),
        description: intention.description.clone(),
        status,
        creator_name,
        creator_letter,
        creator_color_class,
        proofs,
        posted_ago: time_ago(intention.created_at_millis),
        heat,
        attention_peers,
        total_attention_duration,
        pledged_tokens,
        unblessed_event_indices,
        intention_id,
        creator: intention.creator,
    })
}

/// Build token cards for the local member's wallet.
pub async fn build_member_tokens(
    home: &HomeRealm,
    member_id: MemberId,
    local_name: &str,
) -> Vec<TokenCardData> {
    let token_doc = match home.document::<TokenOfGratitudeDocument>("_tokens").await {
        Ok(d) => d,
        Err(_) => return Vec::new(),
    };
    let attention_doc = home.document::<AttentionDocument>("attention").await.ok();

    let tokens: Vec<TokenOfGratitude> = {
        let data = token_doc.read().await;
        data.tokens_for_steward(&member_id)
            .into_iter()
            .cloned()
            .collect()
    };

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

    let mut cards = Vec::new();
    for (idx, t) in tokens.iter().enumerate() {
        let id_hex: String = t.id.iter().map(|b| format!("{b:02x}")).collect();
        let source_title = intention_titles
            .get(&t.source_intention_id)
            .cloned()
            .unwrap_or_else(|| "Unknown intention".to_string());
        let pledged_to = t
            .pledged_to
            .as_ref()
            .and_then(|iid| intention_titles.get(iid).cloned());

        let (blesser_name, blesser_letter, blesser_color_class) =
            member_display(&t.blesser, &member_id, local_name, idx);
        let (current_holder_name, current_holder_letter, current_holder_color_class) =
            member_display(&t.steward, &member_id, local_name, idx + 1);

        let steward_chain: Vec<StewardChainDot> = t
            .steward_chain
            .iter()
            .enumerate()
            .map(|(i, mid)| {
                let (name, letter, color) = member_display(mid, &member_id, local_name, i);
                StewardChainDot {
                    letter,
                    color_class: color,
                    name,
                }
            })
            .collect();

        let attention_duration = if let Some(ref adoc) = attention_doc {
            let adata = adoc.read().await;
            let attn = adata.intention_attention(&t.source_intention_id, None);
            if attn.total_attention_millis > 0 {
                format_duration_secs(attn.total_attention_millis / 1000)
            } else {
                "\u{2014}".to_string()
            }
        } else {
            "\u{2014}".to_string()
        };

        cards.push(TokenCardData {
            id: id_hex,
            raw_id: t.id,
            blessing_source: format!("from {} on '{}'", blesser_name, source_title),
            attention_duration,
            pledged_to,
            steward_chain_len: t.steward_chain.len(),
            blesser_name,
            blesser_letter,
            blesser_color_class,
            source_intention_title: source_title,
            current_holder_name,
            current_holder_letter,
            current_holder_color_class,
            created_ago: time_ago(t.created_at_millis),
            steward_chain,
        });
    }
    cards
}

/// Build intention cards from all DM realms (community intentions).
pub async fn build_community_intention_cards(
    network: &IndrasNetwork,
    member_id: MemberId,
    local_name: &str,
) -> Vec<IntentionCardData> {
    let mut cards = Vec::new();
    let realm_ids = network.conversation_realms();

    for realm_id in realm_ids {
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
        for (idx, intention) in data.intentions.iter().enumerate() {
            if intention.deleted || intention.creator == member_id {
                continue;
            }
            let id_hex: String = intention.id.iter().map(|b| format!("{b:02x}")).collect();
            let (creator_name, creator_letter, creator_color_class) =
                member_display(&intention.creator, &member_id, local_name, idx);

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

            cards.push(IntentionCardData {
                id: id_hex,
                raw_id: intention.id,
                kind: intention.kind.clone(),
                title: intention.title.clone(),
                description: intention.description.clone(),
                proof_count: intention.claims.len(),
                token_count: 0,
                attention_duration: String::new(),
                heat: 0.0,
                is_complete: intention.is_complete(),
                creator_name,
                creator_letter,
                creator_color_class,
                status,
                posted_ago: time_ago(intention.created_at_millis),
            });
        }
    }

    cards
}
