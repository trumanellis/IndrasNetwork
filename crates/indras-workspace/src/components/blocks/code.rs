use dioxus::prelude::*;

#[component]
pub fn CodeBlock(content: String, language: Option<String>) -> Element {
    rsx! {
        div {
            class: "block",
            div {
                class: "block-code",
                "{content}"
            }
        }
    }
}
