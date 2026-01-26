//! Theme system for Realm Viewer
//!
//! Provides theme switching between Quiet Protocol (dark) and Light modes.

use dioxus::prelude::*;

/// Available themes for the application
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Theme {
    #[default]
    QuietProtocol,
    Light,
}

impl Theme {
    /// Returns the CSS data-theme attribute value
    pub fn css_value(&self) -> &'static str {
        match self {
            Theme::QuietProtocol => "quiet-protocol",
            Theme::Light => "light",
        }
    }

    /// Returns the display name for the theme
    pub fn display_name(&self) -> &'static str {
        match self {
            Theme::QuietProtocol => "Quiet Protocol",
            Theme::Light => "Light",
        }
    }

    /// Returns all available themes
    pub fn all() -> &'static [Theme] {
        &[Theme::QuietProtocol, Theme::Light]
    }
}

/// Global signal for current theme
pub static CURRENT_THEME: GlobalSignal<Theme> = GlobalSignal::new(|| Theme::default());

/// Themed root wrapper component
#[component]
pub fn ThemedRoot(children: Element) -> Element {
    let theme = CURRENT_THEME.read();

    rsx! {
        div {
            class: "themed-root",
            "data-theme": "{theme.css_value()}",
            {children}
        }
    }
}

/// Theme switcher dropdown component
#[component]
pub fn ThemeSwitcher() -> Element {
    let mut theme = CURRENT_THEME.write();

    rsx! {
        div { class: "theme-switcher",
            select {
                value: "{theme.css_value()}",
                onchange: move |evt| {
                    let value = evt.value();
                    *theme = match value.as_str() {
                        "light" => Theme::Light,
                        _ => Theme::QuietProtocol,
                    };
                },
                for t in Theme::all() {
                    option {
                        value: "{t.css_value()}",
                        selected: *t == *theme,
                        "{t.display_name()}"
                    }
                }
            }
        }
    }
}
