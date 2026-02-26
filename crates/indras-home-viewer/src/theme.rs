//! Skin system for the home realm viewer.

use dioxus::prelude::*;

/// Available skins for the home viewer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Skin {
    #[default]
    Technical,
}

impl Skin {
    /// Returns the CSS data-skin value for this skin.
    pub fn css_value(&self) -> &'static str {
        match self {
            Skin::Technical => "technical",
        }
    }

    /// Returns the display name for this skin.
    pub fn display_name(&self) -> &'static str {
        match self {
            Skin::Technical => "Technical",
        }
    }
}

/// Global signal for the current skin.
pub static CURRENT_SKIN: GlobalSignal<Skin> = GlobalSignal::new(|| Skin::default());

/// Root component that applies the current skin.
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
