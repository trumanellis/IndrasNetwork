use dioxus::prelude::*;

#[component]
pub fn TokenCardWidget(
    name: String,
    description: String,
    hours: String,
    earned_date: String,
    selected: bool,
    on_click: EventHandler<()>,
) -> Element {
    let card_class = if selected {
        "token-card selected"
    } else {
        "token-card"
    };

    rsx! {
        div {
            class: "{card_class}",
            onclick: move |_| on_click.call(()),

            div {
                class: "token-header",
                div { class: "token-name", "{name}" }
                div { class: "token-hours", "{hours}" }
            }
            div { class: "token-meta", "Earned {earned_date}" }
            div { class: "token-description", "{description}" }
        }
    }
}
