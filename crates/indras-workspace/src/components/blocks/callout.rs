use dioxus::prelude::*;
use indras_ui::render_markdown_to_html;

#[component]
pub fn CalloutBlock(content: String) -> Element {
    let html = render_markdown_to_html(&content);
    rsx! {
        div {
            class: "block",
            div {
                class: "block-callout",
                dangerous_inner_html: "{html}",
            }
        }
    }
}
