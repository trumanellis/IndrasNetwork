//! Skin system for Indras Network applications.
//!
//! Provides 7 skins: Technical, Organic, Botanical, Jewels, Modern, Contemplative, and Solarpunk.
//! Each skin is a self-contained visual identity: colors, fonts, border radii, accent.

use dioxus::prelude::*;

/// Available skins for the application.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Skin {
    #[default]
    Technical,
    Organic,
    Botanical,
    Jewels,
    Modern,
    Contemplative,
    Solarpunk,
}

impl Skin {
    /// Returns the CSS data-skin attribute value.
    pub fn css_value(&self) -> &'static str {
        match self {
            Skin::Technical => "technical",
            Skin::Organic => "organic",
            Skin::Botanical => "botanical",
            Skin::Jewels => "jewels",
            Skin::Modern => "modern",
            Skin::Contemplative => "contemplative",
            Skin::Solarpunk => "solarpunk",
        }
    }

    /// Returns the display name for the skin.
    pub fn display_name(&self) -> &'static str {
        match self {
            Skin::Technical => "Technical",
            Skin::Organic => "Organic",
            Skin::Botanical => "Botanical",
            Skin::Jewels => "Jewels",
            Skin::Modern => "Modern",
            Skin::Contemplative => "Contemplative",
            Skin::Solarpunk => "Solarpunk",
        }
    }

    /// Returns all available skins.
    pub fn all() -> &'static [Skin] {
        &[
            Skin::Technical,
            Skin::Organic,
            Skin::Botanical,
            Skin::Jewels,
            Skin::Modern,
            Skin::Contemplative,
            Skin::Solarpunk,
        ]
    }
}

/// Global signal for current skin.
pub static CURRENT_SKIN: GlobalSignal<Skin> = GlobalSignal::new(|| Skin::default());

/// Themed root wrapper component.
#[component]
pub fn ThemedRoot(children: Element) -> Element {
    let skin = *CURRENT_SKIN.read();

    rsx! {
        div {
            class: "themed-root",
            "data-skin": "{skin.css_value()}",
            {children}
        }
    }
}

/// Skin switcher dropdown component.
#[component]
pub fn SkinSwitcher() -> Element {
    let current_skin = *CURRENT_SKIN.read();

    rsx! {
        div { class: "skin-switcher",
            select {
                value: "{current_skin.css_value()}",
                onchange: move |evt| {
                    let value = evt.value();
                    let new_skin = match value.as_str() {
                        "organic" => Skin::Organic,
                        "botanical" => Skin::Botanical,
                        "jewels" => Skin::Jewels,
                        "modern" => Skin::Modern,
                        "contemplative" => Skin::Contemplative,
                        "solarpunk" => Skin::Solarpunk,
                        _ => Skin::Technical,
                    };
                    *CURRENT_SKIN.write() = new_skin;
                },
                for s in Skin::all() {
                    option {
                        value: "{s.css_value()}",
                        selected: *s == current_skin,
                        "{s.display_name()}"
                    }
                }
            }
        }
    }
}
