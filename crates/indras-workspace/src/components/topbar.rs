//! Topbar with breadcrumbs and action buttons.

use dioxus::prelude::*;
use crate::state::navigation::BreadcrumbEntry;

#[component]
pub fn Topbar(
    breadcrumbs: Vec<BreadcrumbEntry>,
    steward_name: Option<String>,
    on_crumb_click: EventHandler<String>,
    on_toggle_detail: EventHandler<()>,
    on_toggle_sidebar: EventHandler<()>,
    on_share: Option<EventHandler<()>>,
) -> Element {
    rsx! {
        div {
            class: "topbar",
            button {
                class: "topbar-hamburger",
                onclick: move |_| on_toggle_sidebar.call(()),
                "\u{2630}"
            }
            div {
                class: "breadcrumbs",
                for (i, crumb) in breadcrumbs.iter().enumerate() {
                    {
                        let is_last = i == breadcrumbs.len() - 1;
                        let crumb_class = if is_last { "crumb current" } else { "crumb" };
                        let id = crumb.id.clone();
                        rsx! {
                            if i > 0 {
                                span { class: "crumb-sep", "\u{203A}" }
                            }
                            span {
                                class: "{crumb_class}",
                                onclick: move |_| on_crumb_click.call(id.clone()),
                                "{crumb.label}"
                            }
                        }
                    }
                }
            }
            div {
                class: "topbar-actions",
                if let Some(name) = &steward_name {
                    div {
                        class: "steward-badge",
                        span { class: "dot" }
                        span { "Steward: {name}" }
                    }
                }
                button {
                    class: "topbar-btn desktop-only",
                    onclick: move |_| on_toggle_detail.call(()),
                    "\u{2699} ",
                    span { class: "btn-label", "Properties" }
                }
                button {
                    class: "topbar-btn primary",
                    onclick: move |_| {
                        if let Some(handler) = &on_share {
                            handler.call(());
                        }
                    },
                    "\u{1F465} ",
                    span { class: "btn-label", "Share" }
                }
            }
        }
    }
}
