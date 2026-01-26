//! Theme system for Realm Viewer
//!
//! Provides theme switching between 4 themes: Quiet Protocol, Mystic, Neon, and Light.

use dioxus::prelude::*;

/// Available themes for the application
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Theme {
    #[default]
    QuietProtocol,
    Mystic,
    Neon,
    Light,
}

impl Theme {
    /// Returns the CSS data-theme attribute value
    pub fn css_value(&self) -> &'static str {
        match self {
            Theme::QuietProtocol => "quiet-protocol",
            Theme::Mystic => "mystic",
            Theme::Neon => "neon",
            Theme::Light => "light",
        }
    }

    /// Returns the display name for the theme
    pub fn display_name(&self) -> &'static str {
        match self {
            Theme::QuietProtocol => "Quiet Protocol",
            Theme::Mystic => "Mystic Terminal",
            Theme::Neon => "Neon",
            Theme::Light => "Light",
        }
    }

    /// Returns all available themes
    pub fn all() -> &'static [Theme] {
        &[Theme::QuietProtocol, Theme::Mystic, Theme::Neon, Theme::Light]
    }
}

/// Global signal for current theme
pub static CURRENT_THEME: GlobalSignal<Theme> = GlobalSignal::new(|| Theme::default());

/// Themed root wrapper component
#[component]
pub fn ThemedRoot(children: Element) -> Element {
    // Copy the value to avoid holding borrow across children render
    let theme = *CURRENT_THEME.read();

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
    // Read current theme (copy value to avoid borrow conflicts)
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
