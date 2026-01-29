//! Omniperspective Dashboard Components
//!
//! Displays all member POV dashboards simultaneously in a multi-column grid.
//! Each column shows a compact view of one member's perspective.

use dioxus::prelude::*;

use crate::state::{
    member_name, short_id, AppState, QuestAttention, QuestInfo,
};
use crate::theme::{ThemeSwitcher, ThemedRoot};

use super::{
    member_color_class,
    FloatingControlBar, MarkdownPreviewOverlay, ProofNarrativeOverlay,
    MyChatPanel, MyActivity, TokensOfGratitudePanel,
    PreviewFile, PreviewViewMode, PreviewContext,
    ProofNarrativeData, ProofNarrativeContext,
};

// ============================================================================
// OMNI APP - Top-level component
// ============================================================================

/// Top-level omni viewer component
#[component]
pub fn OmniApp(state: Signal<AppState>) -> Element {
    // Preview overlay state
    let preview_open = use_signal(|| false);
    let preview_file = use_signal(|| None::<PreviewFile>);
    let preview_mode = use_signal(|| PreviewViewMode::Rendered);

    // Proof narrative overlay state
    let proof_narrative_open = use_signal(|| false);
    let proof_narrative_data = use_signal(|| None::<ProofNarrativeData>);
    let proof_narrative_mode = use_signal(|| PreviewViewMode::Rendered);

    // Provide preview context to children
    use_context_provider(|| PreviewContext {
        is_open: preview_open,
        file: preview_file,
        view_mode: preview_mode,
    });

    // Provide proof narrative context to children
    use_context_provider(|| ProofNarrativeContext {
        is_open: proof_narrative_open,
        data: proof_narrative_data,
    });

    rsx! {
        ThemedRoot {
            ThemeSwitcher {}
            OmniDashboard { state }
            FloatingControlBar { state }
            MarkdownPreviewOverlay {
                is_open: preview_open,
                file: preview_file,
                view_mode: preview_mode,
            }
            ProofNarrativeOverlay {
                is_open: proof_narrative_open,
                data: proof_narrative_data,
                view_mode: proof_narrative_mode,
            }
        }
    }
}

// ============================================================================
// OMNI DASHBOARD - Grid layout
// ============================================================================

/// Grid layout showing all member POV panels
#[component]
fn OmniDashboard(state: Signal<AppState>) -> Element {
    let members = state.read().all_members();
    let column_count = members.len().max(1);

    rsx! {
        div {
            class: "omni-dashboard",
            style: "--omni-columns: {column_count}",
            for member in members.iter() {
                CompactPOVPanel {
                    state,
                    member: member.clone(),
                }
            }
            if members.is_empty() {
                div { class: "omni-empty",
                    div { class: "empty-state", "Waiting for members..." }
                }
            }
        }
    }
}

// ============================================================================
// COMPACT POV PANEL - Single member column
// ============================================================================

/// Compact single-column POV panel for one member
#[component]
fn CompactPOVPanel(state: Signal<AppState>, member: String) -> Element {
    let color_class = member_color_class(&member);

    rsx! {
        div { class: "omni-panel {color_class}",
            CompactProfileHeader { state, member: member.clone() }
            div { class: "omni-panel-body",
                CompactQuestsList { state, member: member.clone() }
                TokensOfGratitudePanel { state, member: member.clone() }
                MyChatPanel { state, member: member.clone() }
                MyActivity { state, member: member.clone() }
            }
        }
    }
}

// ============================================================================
// COMPACT PROFILE HEADER - Member identity row
// ============================================================================

/// Compact horizontal header showing member avatar, name, and current focus
#[component]
fn CompactProfileHeader(state: Signal<AppState>, member: String) -> Element {
    let name = member_name(&member);
    let initial = name.chars().next().unwrap_or('?');
    let color_class = member_color_class(&member);

    let state_read = state.read();
    let current_focus = state_read.attention.focus_for_member(&member).cloned();
    let current_focus_title = current_focus.as_ref().and_then(|qid| {
        state_read.quests.quests.get(qid).map(|q| q.title.clone())
    });

    // Member stats
    let stats = state_read.stats_for_member(&member);

    rsx! {
        div { class: "compact-profile-header",
            div { class: "compact-profile-identity",
                div { class: "compact-avatar {color_class}", "{initial}" }
                div { class: "compact-profile-info",
                    div { class: "compact-profile-name", "{name}" }
                    if let Some(ref title) = current_focus_title {
                        div { class: "compact-focus-quest", "{title}" }
                    } else {
                        div { class: "compact-focus-none", "No focus" }
                    }
                }
            }
            div { class: "compact-profile-stats",
                span { class: "compact-stat", "{stats.realms_count}R" }
                span { class: "compact-stat", "{stats.quests_created}Q" }
                span { class: "compact-stat", "{stats.events_count}E" }
            }
        }
    }
}

// ============================================================================
// COMPACT QUESTS LIST - Member-filtered quest list
// ============================================================================

/// Quest list filtered to realms where this member participates
#[component]
fn CompactQuestsList(state: Signal<AppState>, member: String) -> Element {
    let state_read = state.read();

    // Get realms this member belongs to
    let member_realm_ids: Vec<String> = state_read
        .realms
        .realms_for_member(&member)
        .iter()
        .map(|r| r.realm_id.clone())
        .collect();

    // Get quests from those realms with attention data
    let quests_with_attention: Vec<(QuestInfo, QuestAttention, Option<String>)> = {
        let attention_rankings = state_read.attention.quests_by_attention();
        let mut result = Vec::new();

        // First add quests that have attention data and belong to member's realms
        for qa in &attention_rankings {
            if let Some(quest) = state_read.quests.quests.get(&qa.quest_id) {
                if member_realm_ids.contains(&quest.realm_id) {
                    let realm_name = state_read.realms.realms.values()
                        .find(|r| r.realm_id == quest.realm_id)
                        .map(|r| {
                            let names: Vec<String> = r.members.iter().take(2).map(|m| member_name(m)).collect();
                            if names.is_empty() { short_id(&r.realm_id) } else { names.join("+") }
                        });
                    result.push((quest.clone(), qa.clone(), realm_name));
                }
            }
        }

        // Add quests without attention data from member's realms
        for quest in state_read.quests.quests.values() {
            if member_realm_ids.contains(&quest.realm_id)
                && !result.iter().any(|(q, _, _)| q.quest_id == quest.quest_id)
            {
                let realm_name = state_read.realms.realms.values()
                    .find(|r| r.realm_id == quest.realm_id)
                    .map(|r| {
                        let names: Vec<String> = r.members.iter().take(2).map(|m| member_name(m)).collect();
                        if names.is_empty() { short_id(&r.realm_id) } else { names.join("+") }
                    });
                result.push((
                    quest.clone(),
                    QuestAttention {
                        quest_id: quest.quest_id.clone(),
                        total_attention_ms: 0,
                        by_member: std::collections::HashMap::new(),
                        currently_focusing: Vec::new(),
                    },
                    realm_name,
                ));
            }
        }

        result
    };

    let count = quests_with_attention.len();
    let max_attention = quests_with_attention
        .iter()
        .map(|(_, qa, _)| qa.total_attention_ms)
        .max()
        .unwrap_or(1)
        .max(1);

    rsx! {
        div { class: "compact-quests",
            div { class: "compact-section-header",
                span { class: "compact-section-title", "Quests" }
                span { class: "compact-section-count", "{count}" }
            }
            div { class: "compact-quest-list",
                for (quest, attention, realm_name) in quests_with_attention.iter() {
                    CompactQuestCard {
                        quest: quest.clone(),
                        attention: attention.clone(),
                        realm_name: realm_name.clone(),
                        max_attention,
                    }
                }
                if count == 0 {
                    div { class: "empty-state compact-empty", "No quests yet" }
                }
            }
        }
    }
}

/// Compact quest card with attention bar
#[component]
fn CompactQuestCard(
    quest: QuestInfo,
    attention: QuestAttention,
    realm_name: Option<String>,
    max_attention: u64,
) -> Element {
    let secs = attention.total_attention_ms as f64 / 1000.0;
    let bar_width_pct = if max_attention > 0 {
        (attention.total_attention_ms as f64 / max_attention as f64 * 100.0).min(100.0)
    } else {
        0.0
    };

    let status_class = quest.status.css_class();
    let status_text = quest.status.display_name();

    rsx! {
        div { class: "compact-quest-card",
            div { class: "compact-quest-header",
                span { class: "compact-quest-title", "{quest.title}" }
                span { class: "quest-status-badge {status_class}", "{status_text}" }
            }
            div { class: "compact-quest-attention",
                div { class: "attention-bar",
                    div {
                        class: "attention-bar-fill",
                        style: "width: {bar_width_pct}%"
                    }
                }
                span { class: "attention-value compact-attention-value", "{secs:.1}s" }
            }
            if !attention.currently_focusing.is_empty() {
                div { class: "compact-quest-focusers",
                    for fmember in &attention.currently_focusing {
                        {
                            let fname = member_name(fmember);
                            let finitial = fname.chars().next().unwrap_or('?');
                            let fcolor = member_color_class(fmember);

                            rsx! {
                                span {
                                    class: "focuser-dot {fcolor}",
                                    title: "{fname}",
                                    "{finitial}"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
