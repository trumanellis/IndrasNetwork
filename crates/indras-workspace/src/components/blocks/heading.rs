use dioxus::prelude::*;

#[component]
pub fn HeadingBlock(level: u8, content: String) -> Element {
    rsx! {
        div {
            class: "block",
            match level {
                1 => rsx! { h1 { "{content}" } },
                3 => rsx! { h3 { "{content}" } },
                4 => rsx! { h4 { "{content}" } },
                _ => rsx! { h2 { "{content}" } },
            }
        }
    }
}
