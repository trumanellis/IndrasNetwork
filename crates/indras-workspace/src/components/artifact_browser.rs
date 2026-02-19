//! Artifact browser — 3-column location-based view (Local / Global / Digital).

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
    let mut selected: Signal<Option<BrowsableArtifact>> = use_signal(|| None);

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
    let mut local: Vec<&BrowsableArtifact> = Vec::new();
    let mut global: Vec<&BrowsableArtifact> = Vec::new();
    let mut digital: Vec<&BrowsableArtifact> = Vec::new();

    for a in &filtered {
        match a.distance_km {
            Some(d) if d <= radius_km => local.push(a),
            Some(_) => global.push(a),
            None => digital.push(a),
        }
    }

    local.sort_by(|a, b| {
        a.distance_km
            .partial_cmp(&b.distance_km)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    global.sort_by(|a, b| {
        a.distance_km
            .partial_cmp(&b.distance_km)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    digital.sort_by(|a, b| a.info.name.cmp(&b.info.name));

    let local_infos: Vec<ArtifactDisplayInfo> = local.iter().map(|a| a.info.clone()).collect();
    let global_infos: Vec<ArtifactDisplayInfo> = global.iter().map(|a| a.info.clone()).collect();
    let digital_infos: Vec<ArtifactDisplayInfo> = digital.iter().map(|a| a.info.clone()).collect();

    let local_count = local_infos.len();
    let global_count = global_infos.len();
    let digital_count = digital_infos.len();

    let artifacts_for_click = artifacts.clone();
    let on_card_click = EventHandler::new(move |info: ArtifactDisplayInfo| {
        if let Some(ba) = artifacts_for_click.iter().find(|a| a.info.id == info.id) {
            selected.set(Some(ba.clone()));
        }
    });

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

                        // Local column
                        div {
                            class: "artifact-browser-column",
                            div {
                                class: "artifact-browser-column-header",
                                "Local"
                                span { class: "artifact-browser-column-count", "{local_count}" }
                            }
                            if local_infos.is_empty() {
                                div { class: "artifact-browser-empty", "No local artifacts" }
                            } else {
                                ArtifactGallery { artifacts: local_infos, on_click: on_card_click }
                            }
                        }

                        // Global column
                        div {
                            class: "artifact-browser-column",
                            div {
                                class: "artifact-browser-column-header",
                                "Global"
                                span { class: "artifact-browser-column-count", "{global_count}" }
                            }
                            if global_infos.is_empty() {
                                div { class: "artifact-browser-empty", "No global artifacts" }
                            } else {
                                ArtifactGallery { artifacts: global_infos, on_click: on_card_click }
                            }
                        }

                        // Digital column
                        div {
                            class: "artifact-browser-column",
                            div {
                                class: "artifact-browser-column-header",
                                "Digital"
                                span { class: "artifact-browser-column-count", "{digital_count}" }
                            }
                            if digital_infos.is_empty() {
                                div { class: "artifact-browser-empty", "No digital artifacts" }
                            } else {
                                ArtifactGallery { artifacts: digital_infos, on_click: on_card_click }
                            }
                        }
                    }
                }
            }

            // Detail modal
            if let Some(ref artifact) = *selected.read() {
                ArtifactDetailModal {
                    artifact: artifact.clone(),
                    on_close: move |_| selected.set(None),
                }
            }
        }
    }
}

/// Modal overlay showing full artifact details.
#[component]
fn ArtifactDetailModal(
    artifact: BrowsableArtifact,
    on_close: EventHandler<()>,
) -> Element {
    let info = &artifact.info;
    let icon = info.icon();
    let size_str = info.formatted_size();
    let status_label = info.status.label();
    let status_class = match info.status {
        indras_ui::artifact_display::ArtifactDisplayStatus::Active => "status-active",
        indras_ui::artifact_display::ArtifactDisplayStatus::Recalled => "status-recalled",
        indras_ui::artifact_display::ArtifactDisplayStatus::Transferred => "status-transferred",
        indras_ui::artifact_display::ArtifactDisplayStatus::Expired => "status-expired",
    };
    let has_image = info.has_displayable_image() && info.data_url.is_some();
    let owner = info.owner_label.clone().unwrap_or_else(|| "Unknown".to_string());
    let mime = info.mime_type.clone().unwrap_or_else(|| "unknown".to_string());
    let distance_str = match artifact.distance_km {
        Some(d) => format!("{d:.1} km"),
        None => "N/A (digital)".to_string(),
    };

    rsx! {
        div {
            class: "artifact-detail-overlay",
            onclick: move |_| on_close.call(()),

            div {
                class: "artifact-detail-modal",
                onclick: move |evt| evt.stop_propagation(),

                // Header
                div {
                    class: "artifact-detail-header",
                    div { class: "artifact-detail-title", "{info.name}" }
                    button {
                        class: "artifact-detail-close",
                        onclick: move |_| on_close.call(()),
                        "\u{2715}"
                    }
                }

                // Body
                div {
                    class: "artifact-detail-body",

                    // Preview
                    div {
                        class: "artifact-detail-preview",
                        if has_image {
                            if let Some(ref url) = info.data_url {
                                img { src: "{url}", alt: "{info.name}" }
                            }
                        } else {
                            span { class: "artifact-detail-preview-icon", "{icon}" }
                        }
                    }

                    // Properties
                    div {
                        class: "artifact-detail-props",
                        div {
                            class: "artifact-detail-prop",
                            span { class: "artifact-detail-prop-key", "Name" }
                            span { class: "artifact-detail-prop-val", "{info.name}" }
                        }
                        div {
                            class: "artifact-detail-prop",
                            span { class: "artifact-detail-prop-key", "Size" }
                            span { class: "artifact-detail-prop-val", "{size_str}" }
                        }
                        div {
                            class: "artifact-detail-prop",
                            span { class: "artifact-detail-prop-key", "MIME Type" }
                            span { class: "artifact-detail-prop-val", "{mime}" }
                        }
                        div {
                            class: "artifact-detail-prop",
                            span { class: "artifact-detail-prop-key", "Status" }
                            span { class: "artifact-detail-prop-val {status_class}", "{status_label}" }
                        }
                        div {
                            class: "artifact-detail-prop",
                            span { class: "artifact-detail-prop-key", "ID" }
                            span { class: "artifact-detail-prop-val", "{info.id}" }
                        }
                        div {
                            class: "artifact-detail-prop",
                            span { class: "artifact-detail-prop-key", "Owner" }
                            span { class: "artifact-detail-prop-val", "{owner}" }
                        }
                        div {
                            class: "artifact-detail-prop",
                            span { class: "artifact-detail-prop-key", "Grants" }
                            span { class: "artifact-detail-prop-val", "{info.grant_count}" }
                        }
                        div {
                            class: "artifact-detail-prop",
                            span { class: "artifact-detail-prop-key", "Origin" }
                            span { class: "artifact-detail-prop-val", "{artifact.origin_label}" }
                        }
                        div {
                            class: "artifact-detail-prop",
                            span { class: "artifact-detail-prop-key", "Distance" }
                            span { class: "artifact-detail-prop-val", "{distance_str}" }
                        }
                    }
                }
            }
        }
    }
}
