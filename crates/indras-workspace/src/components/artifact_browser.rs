//! Artifact browser — 3-column location-based view (Nearby / Distant / Untagged).

use dioxus::prelude::*;
use indras_ui::artifact_display::{ArtifactDisplayInfo, ArtifactGallery};

/// MIME-type filter category.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum MimeCategory {
    #[default]
    All,
    Documents,
    Images,
    Code,
    Audio,
    Video,
}

impl MimeCategory {
    /// All selectable categories in display order.
    pub fn all() -> &'static [MimeCategory] {
        &[
            MimeCategory::All,
            MimeCategory::Documents,
            MimeCategory::Images,
            MimeCategory::Code,
            MimeCategory::Audio,
            MimeCategory::Video,
        ]
    }

    /// Label for the filter chip.
    pub fn label(&self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Documents => "Docs",
            Self::Images => "Images",
            Self::Code => "Code",
            Self::Audio => "Audio",
            Self::Video => "Video",
        }
    }

    /// Whether an artifact's MIME type matches this category.
    pub fn matches(&self, mime: Option<&str>) -> bool {
        match self {
            Self::All => true,
            Self::Documents => mime.map_or(false, |m| {
                m.starts_with("text/")
                    || m == "application/pdf"
                    || m.contains("document")
                    || m.contains("spreadsheet")
            }),
            Self::Images => mime.map_or(false, |m| m.starts_with("image/")),
            Self::Code => mime.map_or(false, |m| {
                m.starts_with("text/x-") || m.contains("javascript") || m.contains("json")
            }),
            Self::Audio => mime.map_or(false, |m| m.starts_with("audio/")),
            Self::Video => mime.map_or(false, |m| m.starts_with("video/")),
        }
    }
}

/// An artifact with computed distance and origin for the browser view.
#[derive(Clone, Debug, PartialEq)]
pub struct BrowsableArtifact {
    pub info: ArtifactDisplayInfo,
    pub distance_km: Option<f64>,
    /// "Mine" or the peer name this artifact was received from.
    pub origin_label: String,
}

/// 3-column artifact browser view.
#[component]
pub fn ArtifactBrowserView(
    artifacts: Vec<BrowsableArtifact>,
    search_query: String,
    on_search: EventHandler<String>,
    active_filter: MimeCategory,
    on_filter: EventHandler<MimeCategory>,
    radius_km: f64,
    on_radius_change: EventHandler<f64>,
    peer_filter: String,
    on_peer_filter: EventHandler<String>,
    available_peers: Vec<String>,
) -> Element {
    // Filter by search + MIME category + peer origin
    let filtered: Vec<&BrowsableArtifact> = artifacts
        .iter()
        .filter(|a| {
            let name_match = search_query.is_empty()
                || a.info.name.to_lowercase().contains(&search_query.to_lowercase());
            let mime_match = active_filter.matches(a.info.mime_type.as_deref());
            let peer_match = peer_filter.is_empty() || a.origin_label == peer_filter;
            name_match && mime_match && peer_match
        })
        .collect();

    // Partition into 3 columns
    let mut nearby: Vec<&BrowsableArtifact> = Vec::new();
    let mut distant: Vec<&BrowsableArtifact> = Vec::new();
    let mut untagged: Vec<&BrowsableArtifact> = Vec::new();

    for a in &filtered {
        match a.distance_km {
            Some(d) if d <= radius_km => nearby.push(a),
            Some(_) => distant.push(a),
            None => untagged.push(a),
        }
    }

    nearby.sort_by(|a, b| {
        a.distance_km
            .partial_cmp(&b.distance_km)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    distant.sort_by(|a, b| {
        a.distance_km
            .partial_cmp(&b.distance_km)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    untagged.sort_by(|a, b| a.info.name.cmp(&b.info.name));

    let nearby_infos: Vec<ArtifactDisplayInfo> = nearby.iter().map(|a| a.info.clone()).collect();
    let distant_infos: Vec<ArtifactDisplayInfo> = distant.iter().map(|a| a.info.clone()).collect();
    let untagged_infos: Vec<ArtifactDisplayInfo> = untagged.iter().map(|a| a.info.clone()).collect();

    let nearby_count = nearby_infos.len();
    let distant_count = distant_infos.len();
    let untagged_count = untagged_infos.len();

    rsx! {
        div {
            class: "view active",
            div {
                class: "content-scroll",
                div {
                    class: "artifact-browser",
                    div { class: "artifact-browser-title", "Artifacts" }

                    // Search bar
                    div {
                        class: "artifact-browser-search",
                        input {
                            class: "artifact-browser-search-input",
                            placeholder: "Search artifacts...",
                            value: "{search_query}",
                            oninput: move |evt| on_search.call(evt.value()),
                        }
                    }

                    // MIME filter chips
                    div {
                        class: "artifact-browser-filters",
                        for cat in MimeCategory::all().iter() {
                            {
                                let is_active = *cat == active_filter;
                                let chip_class = if is_active {
                                    "artifact-browser-filter-chip active"
                                } else {
                                    "artifact-browser-filter-chip"
                                };
                                let cat_clone = cat.clone();
                                rsx! {
                                    button {
                                        class: "{chip_class}",
                                        onclick: move |_| on_filter.call(cat_clone.clone()),
                                        "{cat.label()}"
                                    }
                                }
                            }
                        }
                    }

                    // Peer filter chips
                    div {
                        class: "artifact-browser-filters",
                        {
                            let is_all = peer_filter.is_empty();
                            let chip_class = if is_all {
                                "artifact-browser-filter-chip active"
                            } else {
                                "artifact-browser-filter-chip"
                            };
                            rsx! {
                                button {
                                    class: "{chip_class}",
                                    onclick: move |_| on_peer_filter.call(String::new()),
                                    "All"
                                }
                            }
                        }
                        for peer in available_peers.iter() {
                            {
                                let is_active = *peer == peer_filter;
                                let chip_class = if is_active {
                                    "artifact-browser-filter-chip active"
                                } else {
                                    "artifact-browser-filter-chip"
                                };
                                let peer_clone = peer.clone();
                                rsx! {
                                    button {
                                        class: "{chip_class}",
                                        onclick: move |_| on_peer_filter.call(peer_clone.clone()),
                                        "{peer}"
                                    }
                                }
                            }
                        }
                    }

                    // Radius slider
                    div {
                        class: "artifact-browser-radius",
                        span { "Radius:" }
                        input {
                            r#type: "range",
                            min: "1",
                            max: "1000",
                            value: "{radius_km}",
                            oninput: move |evt| {
                                if let Ok(v) = evt.value().parse::<f64>() {
                                    on_radius_change.call(v);
                                }
                            },
                        }
                        span { "{radius_km:.0} km" }
                    }

                    // 3-column grid
                    div {
                        class: "artifact-browser-columns",

                        // Nearby column
                        div {
                            class: "artifact-browser-column",
                            div {
                                class: "artifact-browser-column-header",
                                "Nearby"
                                span { class: "artifact-browser-column-count", "{nearby_count}" }
                            }
                            if nearby_infos.is_empty() {
                                div { class: "artifact-browser-empty", "No nearby artifacts" }
                            } else {
                                ArtifactGallery { artifacts: nearby_infos }
                            }
                        }

                        // Distant column
                        div {
                            class: "artifact-browser-column",
                            div {
                                class: "artifact-browser-column-header",
                                "Distant"
                                span { class: "artifact-browser-column-count", "{distant_count}" }
                            }
                            if distant_infos.is_empty() {
                                div { class: "artifact-browser-empty", "No distant artifacts" }
                            } else {
                                ArtifactGallery { artifacts: distant_infos }
                            }
                        }

                        // Untagged column
                        div {
                            class: "artifact-browser-column",
                            div {
                                class: "artifact-browser-column-header",
                                "Untagged"
                                span { class: "artifact-browser-column-count", "{untagged_count}" }
                            }
                            if untagged_infos.is_empty() {
                                div { class: "artifact-browser-empty", "No untagged artifacts" }
                            } else {
                                ArtifactGallery { artifacts: untagged_infos }
                            }
                        }
                    }
                }
            }
        }
    }
}
