//! Scenario Picker - Full-screen grid of available Lua scenarios
//!
//! Shown when the viewer launches without piped stdin (TTY detected).
//! Clicking a scenario card fires `on_select` with the scenario path.

use std::path::PathBuf;

use dioxus::prelude::*;

use crate::events::{discover_scenarios, ScenarioInfo};

/// Full-screen scenario picker component
#[component]
pub fn ScenarioPicker(
    on_select: EventHandler<PathBuf>,
    scenarios_dir: PathBuf,
) -> Element {
    let scenarios = use_memo(move || discover_scenarios(&scenarios_dir));

    let mut search_query = use_signal(String::new);
    let mut filter_sdk_only = use_signal(|| true);

    let query = search_query.read().to_lowercase();
    let sdk_only = *filter_sdk_only.read();

    let filtered: Vec<ScenarioInfo> = scenarios
        .read()
        .iter()
        .filter(|s| {
            if sdk_only && !s.is_sdk {
                return false;
            }
            if query.is_empty() {
                return true;
            }
            s.name.to_lowercase().contains(&query)
                || s.description.to_lowercase().contains(&query)
        })
        .cloned()
        .collect();

    rsx! {
        div { class: "scenario-picker",
            // Header
            div { class: "scenario-picker-header",
                h1 { class: "scenario-picker-title", "Indras Network" }
                p { class: "scenario-picker-subtitle", "Select a scenario to observe" }
            }

            // Search bar
            div { class: "scenario-picker-search-row",
                input {
                    class: "scenario-picker-search",
                    r#type: "text",
                    placeholder: "Search scenarios...",
                    value: "{search_query}",
                    oninput: move |e| search_query.set(e.value()),
                }
            }

            // Filter buttons
            div { class: "scenario-picker-filters",
                button {
                    class: if sdk_only { "scenario-filter-btn active" } else { "scenario-filter-btn" },
                    onclick: move |_| filter_sdk_only.set(true),
                    "SDK Scenarios"
                }
                button {
                    class: if !sdk_only { "scenario-filter-btn active" } else { "scenario-filter-btn" },
                    onclick: move |_| filter_sdk_only.set(false),
                    "All"
                }
            }

            // Grid of scenario cards
            div { class: "scenario-picker-list",
                for scenario in filtered.iter() {
                    {
                        let path = scenario.path.clone();
                        let name = &scenario.name;
                        let desc = &scenario.description;
                        let is_sdk = scenario.is_sdk;
                        rsx! {
                            div {
                                class: "scenario-card",
                                onclick: move |_| on_select.call(path.clone()),
                                div { class: "scenario-card-top",
                                    span { class: "scenario-card-name", "{name}" }
                                    if is_sdk {
                                        span { class: "scenario-card-badge", "SDK" }
                                    }
                                }
                                if !desc.is_empty() {
                                    p { class: "scenario-card-desc", "{desc}" }
                                }
                            }
                        }
                    }
                }
                if filtered.is_empty() {
                    div { class: "scenario-picker-empty",
                        "No scenarios match your search."
                    }
                }
            }
        }
    }
}
