use dioxus::prelude::*;

#[component]
pub fn CodeBlock(content: String, language: Option<String>) -> Element {
    rsx! {
        div {
            class: "block",
            if let Some(ref lang) = language {
                if !lang.is_empty() {
                    div {
                        class: "block-code-lang",
                        "{lang}"
                    }
                }
            }
            div {
                class: "block-code",
                "{content}"
            }
        }
    }
}
