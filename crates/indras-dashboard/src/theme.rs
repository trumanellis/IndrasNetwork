// Theme system for Dioxus Desktop
//
// Uses a wrapper div with data-theme attribute instead of web_sys
// since this is a desktop application.

use dioxus::prelude::*;

/// Available themes
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Theme {
    #[default]
    QuietProtocol,
    Mystic,
    Neon,
    Light,
}

impl Theme {
    /// CSS data-theme attribute value
    pub fn as_str(&self) -> &'static str {
        match self {
            Theme::QuietProtocol => "quiet-protocol",
            Theme::Mystic => "mystic",
            Theme::Neon => "neon",
            Theme::Light => "light",
        }
    }

    /// Display name for UI
    pub fn display_name(&self) -> &'static str {
        match self {
            Theme::QuietProtocol => "Quiet Protocol",
            Theme::Mystic => "Mystic Terminal",
            Theme::Neon => "Neon",
            Theme::Light => "Light",
        }
    }

    /// All available themes
    pub fn all() -> &'static [Theme] {
        &[
            Theme::QuietProtocol,
            Theme::Mystic,
            Theme::Neon,
            Theme::Light,
        ]
    }
}

/// Global theme signal - use this throughout your app
pub static CURRENT_THEME: GlobalSignal<Theme> = Signal::global(Theme::default);

/// Hook to access and modify the current theme
pub fn use_theme() -> (Theme, impl Fn(Theme)) {
    let theme = *CURRENT_THEME.read();
    let set_theme = move |new_theme: Theme| {
        *CURRENT_THEME.write() = new_theme;
    };
    (theme, set_theme)
}

/// Theme switcher component - renders a dropdown for theme selection
#[component]
pub fn ThemeSwitcher() -> Element {
    let current_theme = *CURRENT_THEME.read();

    rsx! {
        div { class: "theme-switcher",
            label { class: "theme-label", "Theme" }
            select {
                class: "theme-select",
                value: current_theme.as_str(),
                onchange: move |e| {
                    let value = e.value();
                    let new_theme = match value.as_str() {
                        "quiet-protocol" => Theme::QuietProtocol,
                        "mystic" => Theme::Mystic,
                        "neon" => Theme::Neon,
                        "light" => Theme::Light,
                        _ => Theme::default(),
                    };
                    *CURRENT_THEME.write() = new_theme;
                },
                for theme in Theme::all() {
                    option {
                        value: theme.as_str(),
                        selected: *theme == current_theme,
                        "{theme.display_name()}"
                    }
                }
            }
        }
    }
}

/// Themed wrapper component - wraps children with data-theme attribute
///
/// Use this at the root of your app to enable theme switching:
/// ```rust
/// rsx! {
///     ThemedRoot {
///         // Your app content here
///     }
/// }
/// ```
#[component]
pub fn ThemedRoot(children: Element) -> Element {
    let theme = *CURRENT_THEME.read();

    rsx! {
        div {
            "data-theme": theme.as_str(),
            style: "min-height: 100vh; width: 100%;",
            {children}
        }
    }
}
