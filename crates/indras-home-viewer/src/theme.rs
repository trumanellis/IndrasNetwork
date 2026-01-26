//! Theme system for the home realm viewer.
//!
//! Implements the SyncEngine Design System v2 "Minimal Terminal" aesthetic.

use dioxus::prelude::*;

/// Available themes for the home viewer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Theme {
    /// SyncEngine Design System v2 - void-black, gold, cyan, moss
    #[default]
    MinimalTerminal,
}

impl Theme {
    /// Returns the CSS class value for this theme.
    pub fn css_value(&self) -> &'static str {
        match self {
            Theme::MinimalTerminal => "minimal-terminal",
        }
    }

    /// Returns the display name for this theme.
    pub fn display_name(&self) -> &'static str {
        match self {
            Theme::MinimalTerminal => "Minimal Terminal",
        }
    }
}

/// Global signal for the current theme.
pub static CURRENT_THEME: GlobalSignal<Theme> = GlobalSignal::new(|| Theme::default());

/// Root component that applies the current theme.
#[component]
pub fn ThemedRoot(children: Element) -> Element {
    let theme = *CURRENT_THEME.read();
    rsx! {
        div {
            class: "themed-root",
            "data-theme": "{theme.css_value()}",
            {children}
        }
    }
}
