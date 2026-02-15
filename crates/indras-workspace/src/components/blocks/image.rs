use dioxus::prelude::*;

#[component]
pub fn ImageBlock(caption: Option<String>) -> Element {
    rsx! {
        div {
            class: "block",
            div {
                class: "block-image",
                div { class: "block-image-placeholder", "\u{1F5FA}" }
                if let Some(cap) = &caption {
                    div { class: "image-caption", "{cap}" }
                }
            }
        }
    }
}
