//! Theme support for the Genesis flow.

use dioxus::prelude::*;

/// The application theme.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Theme {
    #[default]
    MinimalTerminal,
}

impl Theme {
    /// Get the CSS data-theme attribute value.
    pub fn css_value(&self) -> &'static str {
        match self {
            Theme::MinimalTerminal => "minimal-terminal",
        }
    }
}

/// Global theme signal.
pub static CURRENT_THEME: GlobalSignal<Theme> = GlobalSignal::new(|| Theme::default());

/// Themed root wrapper component.
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
