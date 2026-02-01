use crate::models::{ActionStatus, ChatMessage};
use dioxus::{document::eval, prelude::*};

#[component]
pub fn ChatView(
    messages: Signal<Vec<ChatMessage>>,
    last_file_path: Signal<String>,
    on_confirm: EventHandler<usize>,
    on_cancel: EventHandler<usize>,
    on_undo: EventHandler<usize>,
) -> Element {
    use_effect(move || {
        messages.read();
        let _ = eval(
            r#"
            const el = document.getElementById('chat-container');
            if (el) el.scrollTop = el.scrollHeight;
        "#,
        );
    });

    // 克隆一份数据用于渲染，避免持有读锁
    let msgs = messages.read().clone();

    rsx! {
        div { id: "chat-scroll", class: "chat-scroll",
            for msg in msgs.iter() {
                div {
                    class: if msg.is_user { "message msg-user" } else { "message msg-ai" },
                    key: "{msg.id}",

                    div { class: "white-space: pre-wrap;", "{msg.text}" }
                    if let Some(img) = &msg.image {
                        img {
                            class: "msg-image",
                            src: "{img}",
                            style: "max-width: 100%; margin-top: 8px; border-radius: 4px;",
                        }
                    }

                    match msg.status {
                        ActionStatus::WaitingConfirmation => {
                            // 提取 id，确保闭包捕获的是 Copy 后的值，而不是 msg 的引用
                            let id = msg.id;
                            rsx! {
                                div { class: "action-bar",
                                    div { class: "code-preview",
                                        if let Some(code) = &msg.pending_code {
                                            pre { style: "font-size:0.8em; opacity:0.7; max-height:100px; overflow:hidden;",
                                                "{code}"
                                            } // 🔥 使用 move 捕获 id (usize 是 Copy 的) // 🔥 使用 move 捕获 id (usize 是 Copy 的) // 🔥 使用 move 捕获 id (usize 是 Copy 的)  🔥 使用 move 捕获 id (usize 是 Copy 的) // 🔥 使用 move 捕获 id (usize 是 Copy 的)  🔥 使用 move 捕获 id (usize 是 Copy 的)  🔥 使用 move 捕获 id (usize 是 Copy 的)  🔥 使用 move 捕获 id (usize 是 Copy 的) // 🔥 使用 move 捕获 id (usize 是 Copy 的)  🔥 使用 move 捕获 id (usize 是 Copy 的)  🔥 使用 move 捕获 id (usize 是 Copy 的)  🔥 使用 move 捕获 id (usize 是 Copy 的)  🔥 使用 move 捕获 id (usize 是 Copy 的)  🔥 使用 move 捕获 id (usize 是 Copy 的)  🔥 使用 move 捕获 id (usize 是 Copy 的)  🔥 使用 move 捕获 id (usize 是 Copy 的) // 🔥 使用 move 捕获 id (usize 是 Copy 的)  🔥 使用 move 捕获 id (usize 是 Copy 的)  🔥 使用 move 捕获 id (usize 是 Copy 的)  🔥 使用 move 捕获 id (usize 是 Copy 的)  🔥 使用 move 捕获 id (usize 是 Copy 的)  🔥 使用 move 捕获 id (usize 是 Copy 的)  🔥 使用 move 捕获 id (usize 是 Copy 的)  🔥 使用 move 捕获 id (usize 是 Copy 的)  🔥 使用 move 捕获 id (usize 是 Copy 的)  🔥 使用 move 捕获 id (usize 是 Copy 的)  🔥 使用 move 捕获 id (usize 是 Copy 的)  🔥 使用 move 捕获 id (usize 是 Copy 的)  🔥 使用 move 捕获 id (usize 是 Copy 的)  🔥 使用 move 捕获 id (usize 是 Copy 的)  🔥 使用 move 捕获 id (usize 是 Copy 的)  🔥 使用 move 捕获 id (usize 是 Copy 的) // 🔥 使用 move 捕获 id (usize 是 Copy 的)  🔥 使用 move 捕获 id (usize 是 Copy 的)  🔥 使用 move 捕获 id (usize 是 Copy 的)  🔥 使用 move 捕获 id (usize 是 Copy 的)  🔥 使用 move 捕获 id (usize 是 Copy 的)  🔥 使用 move 捕获 id (usize 是 Copy 的)  🔥 使用 move 捕获 id (usize 是 Copy 的)  🔥 使用 move 捕获 id (usize 是 Copy 的)  🔥 使用 move 捕获 id (usize 是 Copy 的)  🔥 使用 move 捕获 id (usize 是 Copy 的)  🔥 使用 move 捕获 id (usize 是 Copy 的)  🔥 使用 move 捕获 id (usize 是 Copy 的)  🔥 使用 move 捕获 id (usize 是 Copy 的)  🔥 使用 move 捕获 id (usize 是 Copy 的)  🔥 使用 move 捕获 id (usize 是 Copy 的)  🔥 使用 move 捕获 id (usize 是 Copy 的)  🔥 使用 move 捕获 id (usize 是 Copy 的)  🔥 使用 move 捕获 id (usize 是 Copy 的)  🔥 使用 move 捕获 id (usize 是 Copy 的)  🔥 使用 move 捕获 id (usize 是 Copy 的)  🔥 使用 move 捕获 id (usize 是 Copy 的)  🔥 使用 move 捕获 id (usize 是 Copy 的)  🔥 使用 move 捕获 id (usize 是 Copy 的)  🔥 使用 move 捕获 id (usize 是 Copy 的)  🔥 使用 move 捕获 id (usize 是 Copy 的)  🔥 使用 move 捕获 id (usize 是 Copy 的)  🔥 使用 move 捕获 id (usize 是 Copy 的)  🔥 使用 move 捕获 id (usize 是 Copy 的)  🔥 使用 move 捕获 id (usize 是 Copy 的)  🔥 使用 move 捕获 id (usize 是 Copy 的)  🔥 使用 move 捕获 id (usize 是 Copy 的)  🔥 使用 move 捕获 id (usize 是 Copy 的)
                                        }
                                    }
                                    div { class: "btn-group",
                                        // 🔥 使用 move 捕获 id (usize 是 Copy 的)
                                        button { class: "confirm-btn", onclick: move |_| on_confirm.call(id), "✅ 执行" }
                                        button { class: "cancel-btn", onclick: move |_| on_cancel.call(id), "🚫 取消" }
                                    }
                                }
                            }
                        }
                        ActionStatus::Running => rsx! {
                            div { class: "status-label running", "⏳ 运行中..." }
                        },
                        ActionStatus::Success => {
                            let id = msg.id;
                            rsx! {
                                if msg.backup_path.is_some() {
                                    div { class: "action-bar",
                                        button { class: "undo-btn", onclick: move |_| on_undo.call(id), "↩️ 撤销" }
                                    }
                                }
                            }
                        }
                        ActionStatus::Error(ref e) => rsx! {
                            div { class: "status-label error", "❌ {e}" }
                        },
                        ActionStatus::Cancelled => rsx! {
                            div { class: "status-label cancelled", "🚫 已取消" }
                        },
                        ActionStatus::Undone => rsx! {
                            div { class: "status-label undone", "↩️ 已撤销" }
                        },
                        _ => rsx! {},
                    }
                }
            }
        }
    }
}
