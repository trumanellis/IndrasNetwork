//! Artifacts gallery component.

use dioxus::prelude::*;

use crate::state::{short_id, AppState, Artifact};

/// Artifacts gallery showing uploaded files in a grid.
#[component]
pub fn ArtifactsGallery(state: Signal<AppState>) -> Element {
    let state_read = state.read();

    rsx! {
        section {
            class: "artifacts-panel",

            div {
                class: "panel-header",
                h2 {
                    class: "panel-title",
                    "Artifacts"
                }
                span {
                    class: "panel-count",
                    "{state_read.artifacts.count()} files"
                }
            }

            div {
                class: "artifacts-grid",

                if state_read.artifacts.artifacts.is_empty() {
                    div {
                        class: "artifacts-empty",
                        p { "No artifacts uploaded yet." }
                    }
                } else {
                    for artifact in state_read.artifacts.artifacts_by_recency().iter().take(8) {
                        ArtifactCard {
                            key: "{artifact.id}",
                            artifact: (*artifact).clone(),
                        }
                    }
                }
            }
        }
    }
}

/// A single artifact card.
#[component]
fn ArtifactCard(artifact: Artifact) -> Element {
    rsx! {
        div {
            class: "artifact-card",

            // Icon/thumbnail area
            div {
                class: "artifact-card-icon",
                "{artifact.icon()}"
            }

            // File info
            div {
                class: "artifact-card-info",

                span {
                    class: "artifact-card-type",
                    "{artifact.file_type()}"
                }

                span {
                    class: "artifact-card-size",
                    "{artifact.size_display()}"
                }
            }

            // Metadata footer
            div {
                class: "artifact-card-footer",

                span {
                    class: "artifact-card-id",
                    "{short_id(&artifact.id)}"
                }

                if artifact.retrieved_count > 0 {
                    span {
                        class: "artifact-card-retrieved",
                        "{artifact.retrieved_count}x"
                    }
                }
            }
        }
    }
}
