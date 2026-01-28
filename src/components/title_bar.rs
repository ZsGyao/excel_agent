use dioxus::{desktop::use_window, prelude::*};

#[component]
pub fn TitleBar() -> Element {
    let window = use_window();

    let window_drag = window.clone();
    let window_hide = window.clone();
    let window_close = window.clone();

    rsx! {
        div {
            class: "title-bar",
            // Drag when mouse down
            onmousedown: move |_| {
                window_drag.drag();
            },

            // Left: Logo or Title
            div { class: "title-text", "Excel Agent" }

            // Right: Control buttons
            div { class: "window-controls",
                // Minimize
                div {
                    class: "control-btn minimize",
                    onmousedown: move |evt| {
                        evt.stop_propagation();
                    },
                    onclick: move |evt| {
                        evt.stop_propagation();
                        // 🔥 核心修改：不是最小化，而是直接隐藏！
                        // 隐藏后，只能通过点击托盘图标找回来
                        window_hide.set_visible(false);
                    },
                    "一"
                }
                // Close
                div {
                    class: "control-btn close",
                    onmousedown: move |evt| {
                         evt.stop_propagation();
                    },
                    onclick: move |_| { window_close.close(); },
                    "✕"
                }
            }
        }
    }
}
