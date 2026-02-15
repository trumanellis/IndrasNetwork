use dioxus::prelude::*;

#[component]
pub fn TodoBlock(text: String, done: bool) -> Element {
    let check_class = if done { "todo-check done" } else { "todo-check" };
    let check_mark = if done { "\u{2713}" } else { "" };
    rsx! {
        div {
            class: "block",
            div {
                class: "block-todo",
                div { class: "{check_class}", "{check_mark}" }
                span { class: "todo-text", "{text}" }
            }
        }
    }
}
