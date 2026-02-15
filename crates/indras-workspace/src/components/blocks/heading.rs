use dioxus::prelude::*;

#[component]
pub fn HeadingBlock(level: u8, content: String) -> Element {
    rsx! {
        div {
            class: "block",
            h2 { "{content}" }
        }
    }
}
