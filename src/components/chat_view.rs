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
    // 自动滚动逻辑
    use_effect(move || {
        messages.read();
        let _ = eval(
            r#"
            setTimeout(() => {
                const el = document.getElementById('chat-scroll');
                if (el) el.scrollTop = el.scrollHeight;
            }, 50);
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
                    div { class: "bubble",
                        // 文本内容
                        div { style: "white-space: pre-wrap;", "{msg.text}" }

                        // 图片内容
                        if let Some(img) = &msg.image {
                            img { class: "msg-image", src: "{img}" }
                        }

                        // 🔥 修复：match 必须包裹在 {} 中
                        {
                            match msg.status {
                                ActionStatus::WaitingConfirmation => {
                                    let id = msg.id;
                                    rsx! {
                                        div { class: "action-bar",
                                            div { class: "code-preview",
                                                "检测到操作指令，请确认："
                                                if let Some(code) = &msg.pending_code {
                                                    pre { style: "font-size:0.8em; opacity:0.8; max-height:150px; overflow:hidden; background:#222; color:#eee; padding:5px; border-radius:4px; margin-top:4px;",
                                                        "{code}"
                                                    }
                                                }
                                            }
                                            div { class: "btn-group",
                                                button { class: "confirm-btn", onclick: move |_| on_confirm.call(id), "✅ 执行" }
                                                button { class: "cancel-btn", onclick: move |_| on_cancel.call(id), "🚫 取消" }
                                            }
                                        }
                                    }
                                }
                                ActionStatus::Running => rsx! {
                                    div { class: "status-label running", "⏳ 正在执行 Python 脚本..." }
                                },
                                ActionStatus::Success => {
                                    let id = msg.id;
                                    rsx! {
                                        if msg.backup_path.is_some() {
                                            div { class: "action-bar",
                                                button { class: "undo-btn", onclick: move |_| on_undo.call(id), "↩️ 撤销此操作" }
                                            }
                                        }
                                    }
                                }
                                ActionStatus::Error(ref e) => rsx! {
                                    div { class: "status-label error", "❌ 错误: {e}" }
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
    }
}
