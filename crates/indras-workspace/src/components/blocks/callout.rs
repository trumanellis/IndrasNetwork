use dioxus::prelude::*;

#[component]
pub fn CalloutBlock(content: String) -> Element {
    rsx! {
        div {
            class: "block",
            div {
                class: "block-callout",
                "{content}"
            }
        }
    }
}
