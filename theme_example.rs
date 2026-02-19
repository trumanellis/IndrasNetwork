// Theme system for Dioxus
//
// 1. Add themes.css to your assets
// 2. Include it in your index.html: <link rel="stylesheet" href="themes.css">
// 3. Use this component and hook

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
        &[Theme::QuietProtocol, Theme::Mystic, Theme::Neon, Theme::Light]
    }
}

/// Global theme signal - use this throughout your app
static CURRENT_THEME: GlobalSignal<Theme> = Signal::global(|| Theme::default());

/// Hook to access and modify the current theme
pub fn use_theme() -> (Theme, impl Fn(Theme)) {
    let theme = *CURRENT_THEME.read();
    let set_theme = move |new_theme: Theme| {
        *CURRENT_THEME.write() = new_theme;
        // Update the DOM attribute
        if let Some(window) = web_sys::window() {
            if let Some(document) = window.document() {
                if let Some(html) = document.document_element() {
                    let _ = html.set_attribute("data-theme", new_theme.as_str());
                }
            }
        }
        // Persist to localStorage
        if let Some(window) = web_sys::window() {
            if let Ok(Some(storage)) = window.local_storage() {
                let _ = storage.set_item("theme", new_theme.as_str());
            }
        }
    };
    (theme, set_theme)
}

/// Initialize theme from localStorage on app start
pub fn init_theme_from_storage() {
    if let Some(window) = web_sys::window() {
        if let Ok(Some(storage)) = window.local_storage() {
            if let Ok(Some(saved)) = storage.get_item("theme") {
                let theme = match saved.as_str() {
                    "quiet-protocol" => Theme::QuietProtocol,
                    "mystic" => Theme::Mystic,
                    "neon" => Theme::Neon,
                    "light" => Theme::Light,
                    _ => Theme::default(),
                };
                *CURRENT_THEME.write() = theme;
                // Apply to DOM
                if let Some(document) = window.document() {
                    if let Some(html) = document.document_element() {
                        let _ = html.set_attribute("data-theme", theme.as_str());
                    }
                }
            }
        }
    }
}

/// Theme switcher component
#[component]
pub fn ThemeSwitcher() -> Element {
    let (current_theme, set_theme) = use_theme();

    rsx! {
        div { class: "theme-switcher",
            for theme in Theme::all() {
                button {
                    class: if *theme == current_theme { "active" } else { "" },
                    onclick: move |_| set_theme(*theme),
                    "{theme.display_name()}"
                }
            }
        }
    }
}

// ============================================
// USAGE EXAMPLE
// ============================================

#[component]
pub fn App() -> Element {
    // Initialize theme on first render
    use_effect(|| {
        init_theme_from_storage();
    });

    rsx! {
        ThemeSwitcher {}

        main {
            // Your app content here
            // Use CSS variables: var(--bg-primary), var(--text-primary), etc.
            h1 { "My Dioxus App" }
            p { "Themed with CSS variables" }
        }
    }
}

// ============================================
// EXAMPLE: Themed component using CSS vars
// ============================================

#[component]
pub fn Card(title: String, children: Element) -> Element {
    rsx! {
        div {
            // These styles use CSS variables from themes.css
            style: "
                background: var(--bg-card);
                color: var(--text-primary);
                border: 1px solid var(--border-color);
                border-radius: var(--border-radius);
                padding: var(--space-6);
                box-shadow: var(--glow-effect);
            ",
            h2 { style: "color: var(--accent-1); font-family: var(--font-heading);",
                "{title}"
            }
            div { style: "color: var(--text-secondary);",
                {children}
            }
        }
    }
}
