use dioxus::prelude::*;

#[component]
pub fn DividerBlock() -> Element {
    rsx! {
        div { class: "block-divider" }
    }
}
