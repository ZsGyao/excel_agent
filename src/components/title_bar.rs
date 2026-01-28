use dioxus::{desktop::use_window, prelude::*};

#[component]
pub fn TitleBar() -> Element {
    let window = use_window();

    rsx! {
        div { 
            class: "title-bar",
            onmousedown: 
        }
    }
}
