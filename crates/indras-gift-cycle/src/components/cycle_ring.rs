//! Compact horizontal 6-stage cycle ring navigation strip.

use dioxus::prelude::*;

use crate::state::CycleStage;

/// Compact horizontal cycle ring — sits below the peer bar.
#[component]
pub fn CycleRing(active_stage: CycleStage, on_stage_click: EventHandler<CycleStage>) -> Element {
    rsx! {
        div { class: "cycle-strip",
            for stage in CycleStage::all() {
                {
                    let is_active = stage == active_stage;
                    let active_class = if is_active { " active" } else { "" };
                    let stage_class = match stage {
                        CycleStage::Intention => "stage-intention",
                        CycleStage::Attention => "stage-attention",
                        CycleStage::Service => "stage-service",
                        CycleStage::Blessing => "stage-blessing",
                        CycleStage::Token => "stage-token",
                        CycleStage::Renewal => "stage-renewal",
                    };
                    let icon = stage.icon();
                    let label = stage.label();
                    let stage_clone = stage.clone();

                    rsx! {
                        div {
                            class: "cycle-strip-node {stage_class}{active_class}",
                            onclick: move |_| on_stage_click.call(stage_clone.clone()),
                            div { class: "strip-node-orb", "{icon}" }
                            div { class: "strip-node-label", "{label}" }
                        }
                        // Arrow between nodes (except after last)
                        if stage != CycleStage::Renewal {
                            div { class: "strip-arrow", "\u{203a}" }
                        }
                    }
                }
            }
        }
    }
}
