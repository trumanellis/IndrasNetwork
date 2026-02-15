//! Floating action button (mobile).

use dioxus::prelude::*;

#[component]
pub fn Fab(on_click: EventHandler<()>) -> Element {
    rsx! {
        button {
            class: "fab",
            onclick: move |_| on_click.call(()),
            "+"
        }
    }
}
