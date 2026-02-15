use dioxus::prelude::*;

#[component]
pub fn TextBlock(content: String) -> Element {
    rsx! {
        div {
            class: "block",
            p { "{content}" }
        }
    }
}
