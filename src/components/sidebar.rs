use crate::models::View;
use dioxus::prelude::*;

#[component]
pub fn Sidebar(current_view: Signal<View>) -> Element {
    rsx! {
        div { class: "sidebar",
            div {
                class: if current_view() == View::Chat { "nav-icon active" } else { "nav-icon" },
                onclick: move |_| current_view.set(View::Chat),
                "💬"
            }
            div {
                class: if current_view() == View::Settings { "nav-icon active" } else { "nav-icon" },
                onclick: move |_| current_view.set(View::Settings),
                "⚙️"
            }
        }
    }
}
