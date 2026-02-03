//! Theme system for Indras Network applications.
//!
//! Provides 5 themes: Quiet Protocol, Mystic, Neon, Light, and Minimal Terminal.

use dioxus::prelude::*;

/// Available themes for the application.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Theme {
    #[default]
    QuietProtocol,
    Mystic,
    Neon,
    Light,
    MinimalTerminal,
}

impl Theme {
    /// Returns the CSS data-theme attribute value.
    pub fn css_value(&self) -> &'static str {
        match self {
            Theme::QuietProtocol => "quiet-protocol",
            Theme::Mystic => "mystic",
            Theme::Neon => "neon",
            Theme::Light => "light",
            Theme::MinimalTerminal => "minimal-terminal",
        }
    }

    /// Returns the display name for the theme.
    pub fn display_name(&self) -> &'static str {
        match self {
            Theme::QuietProtocol => "Quiet Protocol",
            Theme::Mystic => "Mystic Terminal",
            Theme::Neon => "Neon",
            Theme::Light => "Light",
            Theme::MinimalTerminal => "Minimal Terminal",
        }
    }

    /// Returns all available themes.
    pub fn all() -> &'static [Theme] {
        &[
            Theme::QuietProtocol,
            Theme::Mystic,
            Theme::Neon,
            Theme::Light,
            Theme::MinimalTerminal,
        ]
    }
}

/// Global signal for current theme.
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

/// Theme switcher dropdown component.
#[component]
pub fn ThemeSwitcher() -> Element {
    let current_theme = *CURRENT_THEME.read();

    rsx! {
        div { class: "theme-switcher",
            select {
                value: "{current_theme.css_value()}",
                onchange: move |evt| {
                    let value = evt.value();
                    let new_theme = match value.as_str() {
                        "mystic" => Theme::Mystic,
                        "neon" => Theme::Neon,
                        "light" => Theme::Light,
                        "minimal-terminal" => Theme::MinimalTerminal,
                        _ => Theme::QuietProtocol,
                    };
                    *CURRENT_THEME.write() = new_theme;
                },
                for t in Theme::all() {
                    option {
                        value: "{t.css_value()}",
                        selected: *t == current_theme,
                        "{t.display_name()}"
                    }
                }
            }
        }
    }
}
