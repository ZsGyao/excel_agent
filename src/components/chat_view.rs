use crate::models::{ActionStatus, ChatMessage};
use dioxus::{document::eval, prelude::*};

fn clean_text(text: &str) -> String {
    let mut result = String::new();
    let mut in_code = false;
    for line in text.lines() {
        if line.trim().starts_with("```") {
            in_code = !in_code;
            continue;
        }
        if !in_code {
            result.push_str(line);
            result.push('\n');
        }
    }
    result
        .replace("下面是代码", "")
        .replace("Here is the code", "")
        .trim()
        .to_string()
}

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
            r#"setTimeout(() => {
            const el = document.getElementById('chat-scroll');
            if(el) el.scrollTop = el.scrollHeight;
        }, 50);"#,
        );
    });

    let msgs = messages.read().clone();

    // 预渲染
    let rendered_msgs = msgs.iter().map(|msg| {
        let msg_id = msg.id;
        let has_code = msg.pending_code.is_some();
        let is_error = matches!(msg.status, ActionStatus::Error(_));
        let is_undone = matches!(msg.status, ActionStatus::Undone);
        let display_text = clean_text(&msg.text);
        let bubble_class = if is_undone { "bubble undone-state" } else { "bubble" };

        // 底部交互栏逻辑
        let bottom_actions = match msg.status {
            ActionStatus::WaitingConfirmation => rsx! {
                div { style: "margin-top: 10px; border-top: 1px solid #eee; padding-top: 10px;",
                    div { style: "font-weight: bold; font-size: 13px; margin-bottom: 6px;",
                        "⚡ 请确认执行："
                    }
                    div { class: "btn-group",
                        button {
                            class: "confirm-btn",
                            onclick: move |_| on_confirm.call(msg_id),
                            "✅ 立即执行"
                        }
                        button {
                            class: "cancel-btn",
                            onclick: move |_| on_cancel.call(msg_id),
                            "🚫 取消"
                        }
                    }
                }
            },
            // 🔥 新增：报错状态下显示重试按钮，防止死胡同
            ActionStatus::Error(_) => rsx! {
                div { style: "margin-top: 10px; border-top: 1px solid #f8d7da; padding-top: 10px;",
                    div { class: "btn-group",
                        button {
                            class: "confirm-btn",
                            style: "background: #dc3545;", // 红色按钮
                            onclick: move |_| on_confirm.call(msg_id),
                            "🔄 重新尝试"
                        }
                    }
                }
            },
            ActionStatus::Success => {
                if msg.backup_paths.is_some() {
                    rsx! {
                        div { style: "margin-top: 8px; border-top: 1px dashed #ccc; padding-top: 4px;",
                            button {
                                class: "undo-btn",
                                onclick: move |_| on_undo.call(msg_id),
                                "↩️ 撤销 / 回溯到此"
                            }
                        }
                    }
                } else { rsx!{} }
            },
            ActionStatus::Undone => rsx! {
                div { style: "margin-top: 8px; font-size: 11px; color: #999; font-style: italic;",
                    "🚫 此操作已回溯失效"
                }
            },
            _ => rsx! {}
        };

        rsx! {
            div {
                key: "{msg_id}",
                class: if msg.is_user { "message msg-user" } else { "message msg-ai" },

                div { class: "{bubble_class}",
                    // 文本
                    if !display_text.is_empty() {
                        div { style: if is_undone { "white-space: pre-wrap; margin-bottom: 8px; text-decoration: line-through; opacity: 0.7;" } else { "white-space: pre-wrap; margin-bottom: 8px;" },
                            "{display_text}"
                        }
                    }

                    // 思考过程
                    if !msg.is_user && (has_code || is_error) {
                        details {
                            class: "thinking-details",
                            open: if is_undone { "false" } else { "true" },
                            summary { class: "thinking-summary",
                                if is_undone {
                                    "⏹️ 历史操作 (已回溯)"
                                } else {
                                    "▶ 思考过程 (Execution Process)"
                                }
                            }
                            div { class: "thinking-content",
                                if let Some(code) = &msg.pending_code {
                                    pre { style: "font-size: 0.8em; overflow-x: auto; background: #222; color: #eee; padding: 8px; margin: 0;",
                                        "{code}"
                                    }
                                }
                                if let ActionStatus::Error(e) = &msg.status {
                                    div {
                                        class: "status-label error",
                                        style: "white-space: pre-wrap;",
                                        "❌ {e}"
                                    }
                                }
                                if let ActionStatus::Running = &msg.status {
                                    div { class: "status-label running", "⏳ 正在执行..." }
                                }
                            }
                        }
                    }

                    if let Some(img) = &msg.image {
                        img {
                            class: "msg-image",
                            src: "{img}",
                            style: "max-width: 100%; margin-top: 8px; border-radius: 4px;",
                        }
                    }

                    {bottom_actions}
                }
            }
        }
    });

    rsx! {
        div { id: "chat-scroll", class: "chat-scroll", {rendered_msgs} }
    }
}
