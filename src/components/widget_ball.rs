use dioxus::{desktop::use_window, html::HasFileData, prelude::*};

use crate::models::{ActionStatus, ChatMessage, WindowMode};

#[component]
pub fn WidgetBall(
    window_mode: Signal<WindowMode>,
    is_dragging: Signal<bool>,          // Global drag status
    messages: Signal<Vec<ChatMessage>>, // Used for processing files and sending messages
    last_file_path: Signal<String>,
) -> Element {
    let window = use_window();

    // Drag window logical, move the float ball Widget
    let handle_drag_move = move |_| {
        window.drag();
    };

    // Click to switch Main
    let handle_click = move |evt: Event<MouseData>| {
        window_mode.set(WindowMode::Main);
    };

    rsx! {
        div { class: "widget-container",
            // The entire background is transparent, only the sphere is visible
            div {
                class: if is_dragging() { "widget-ball drag-hover" } else { "widget-ball" },
                // Click and drag the window
                onmousedown: handle_drag_move,
                // Right click to switch to main window
                oncontextmenu: move |evt| {
                    evt.prevent_default();
                    window_mode.set(WindowMode::Main);
                },
                // File drag and drop processing
                ondragover: move |evt| {
                    evt.prevent_default();
                    is_dragging.set(true);
                },
                ondragleave: move |evt| {
                    evt.prevent_default();
                    is_dragging.set(false);
                },
                ondrop: move |evt| {
                    evt.prevent_default();
                    is_dragging.set(false);

                    // Handle File
                    let files = evt.data().files();
                    if let Some(first_file) = files.first() {
                        let file_name = first_file.name();
                        let current_dir = std::env::current_dir().unwrap();
                        let full_path = current_dir.join(&file_name).to_str().unwrap().to_string();

                        // Save Path
                        last_file_path.set(full_path);

                        // Send Message
                        let new_id = messages.read().len();
                        messages
                            .write()
                            .push(ChatMessage {
                                id: new_id,
                                text: format!("📂 已通过悬浮球加载: {}", file_name),
                                is_user: false,
                                table: None,
                                temp_id: None,
                                status: ActionStatus::None,
                                image: None,
                            });
                        window_mode.set(WindowMode::Main);
                    }
                },

                // Icon
                if is_dragging() {
                    "📂"
                } else {
                    "🤖"
                }
            }
        }
    }
}
