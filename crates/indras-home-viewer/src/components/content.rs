//! Main content area component.

use dioxus::prelude::*;

use crate::state::AppState;

/// Main content area - currently wraps child components.
/// This file exists for extensibility but the main layout is in app.rs.
#[component]
pub fn ContentArea(state: Signal<AppState>, children: Element) -> Element {
    rsx! {
        div {
            class: "content-area",
            {children}
        }
    }
}

/// Empty state component for when there's no content.
#[component]
pub fn EmptyState(title: String, message: String) -> Element {
    rsx! {
        div {
            class: "empty-state",
            h3 {
                class: "empty-state-title",
                "{title}"
            }
            p {
                class: "empty-state-message",
                "{message}"
            }
        }
    }
}
