// Skin system for Dioxus Desktop
//
// Uses a wrapper div with data-skin attribute for skin switching.

use dioxus::prelude::*;

/// Available skins
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
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
    /// CSS data-skin attribute value
    pub fn as_str(&self) -> &'static str {
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

    /// Display name for UI
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

    /// All available skins
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

/// Global skin signal
pub static CURRENT_SKIN: GlobalSignal<Skin> = Signal::global(Skin::default);

/// Hook to access and modify the current skin
pub fn use_skin() -> (Skin, impl Fn(Skin)) {
    let skin = *CURRENT_SKIN.read();
    let set_skin = move |new_skin: Skin| {
        *CURRENT_SKIN.write() = new_skin;
    };
    (skin, set_skin)
}

/// Skin switcher component
#[component]
pub fn SkinSwitcher() -> Element {
    let current_skin = *CURRENT_SKIN.read();

    rsx! {
        div { class: "skin-switcher",
            label { class: "skin-label", "Skin" }
            select {
                class: "skin-select",
                value: current_skin.as_str(),
                onchange: move |e| {
                    let value = e.value();
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
                for skin in Skin::all() {
                    option {
                        value: skin.as_str(),
                        selected: *skin == current_skin,
                        "{skin.display_name()}"
                    }
                }
            }
        }
    }
}

/// Themed wrapper component - wraps children with data-skin attribute
#[component]
pub fn ThemedRoot(children: Element) -> Element {
    let skin = *CURRENT_SKIN.read();

    rsx! {
        div {
            "data-skin": skin.as_str(),
            style: "min-height: 100vh; width: 100%;",
            {children}
        }
    }
}
