use dioxus::prelude::*;
use indras_ui::render_markdown_to_html;

#[component]
pub fn TextBlock(content: String) -> Element {
    let html = render_markdown_to_html(&content);
    rsx! {
        div {
            class: "block",
            div { dangerous_inner_html: "{html}" }
        }
    }
}
