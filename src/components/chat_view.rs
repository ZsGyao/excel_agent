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

    let msgs = messages.read().clone();

    rsx! {
        div { id: "chat-scroll", class: "chat-scroll",
            for msg in msgs.iter() {
                div {
                    key: "{msg.id}",
                    class: if msg.is_user { "message msg-user" } else { "message msg-ai" },

                    div { class: "bubble",

                        // === 1. 思考过程折叠面板 (仅包含代码和运行日志) ===
                        if !msg.is_user
                            && (msg.pending_code.is_some()
                                || matches!(msg.status, ActionStatus::Running | ActionStatus::Error(_)))
                        {
                            details { class: "thinking-details", open: "true", // 默认展开
                                summary { class: "thinking-summary",
                                    span { class: "arrow-icon", "▶" }
                                    span { "思考过程 (Execution Process)" }
                                }
                                div { class: "thinking-content",
                                    // A. 代码预览
                                    if let Some(code) = &msg.pending_code {
                                        pre { style: "font-size: 0.8em; overflow-x: auto; background: #222; color: #eee; padding: 8px; border-radius: 4px; margin: 0 0 8px 0;",
                                            "{code}"
                                        }
                                    }

                                    // B. 运行状态 / 错误日志 (都在折叠框内)
                                    {
                                        match msg.status {
                                            ActionStatus::Running => rsx! {
                                                div { class: "status-label running", "⏳ 正在操作 Excel..." }
                                            },
                                            ActionStatus::Error(ref e) => rsx! {
                                                div {
                                                    class: "status-label error",
                                                    style: "white-space: pre-wrap; word-break: break-all;",
                                                    "❌ 详细错误日志:\n{e}"
                                                }
                                            },
                                            _ => rsx! {},
                                        }
                                    }
                                }
                            }
                        }

                        // === 2. 核心文本内容 ===
                        // 显示 AI 的回复，或者 "✨ 执行成功" / "🛑 自动修复失败" 的提示
                        if !msg.text.is_empty() {
                            div { style: "white-space: pre-wrap; margin-top: 8px;",
                                "{msg.text}"
                            }
                        }

                        // === 3. 图片内容 ===
                        if let Some(img) = &msg.image {
                            img {
                                class: "msg-image",
                                src: "{img}",
                                style: "max-width: 100%; margin-top: 8px; border-radius: 4px;",
                            }
                        }

                        // === 4. 交互操作区 (放在最外层，方便点击) ===
                        {
                            match msg.status {
                                // 🔥 重点：WaitingConfirmation 的按钮放在这里，绝对不在 details 里！
                                ActionStatus::WaitingConfirmation => {
                                    // ✅ 在代码块内提取 ID，修复编译错误
                                    let id = msg.id;
                                    rsx! {
                                        div { style: "margin-top: 12px; padding-top: 12px; border-top: 1px solid #eee;",
                                            div { style: "font-size: 13px; font-weight: 700; margin-bottom: 8px; color: #333;",
                                                "⚡ 检测到操作指令，请确认："
                                            }
                                            div { class: "btn-group",
                                                button { class: "confirm-btn", onclick: move |_| on_confirm.call(id), "✅ 立即执行" }
                                                button { class: "cancel-btn", onclick: move |_| on_cancel.call(id), "🚫 取消" }
                                            }
                                        }
                                    }
                                }
                                ActionStatus::Success => {
                                    if let Some(_) = &msg.backup_path {
                                        let id = msg.id;
                                        rsx! { // 取消提示
                                            div { style: "margin-top: 8px; border-top: 1px dashed #ccc; padding-top: 4px;",
                                                button {
                                                    class: "undo-btn",
                                                    style: "background: transparent; color: #999; border: none; padding: 0; font-size: 11px; cursor: pointer; text-decoration: underline;",
                                                    onclick: move |_| on_undo.call(id),
                                                    "↩️ 撤销此操作 (需先关闭文件)"
                                                }
                                            }
                                        }
                                    } else {
                                        rsx! {}
                                    }
                                }
                                ActionStatus::Cancelled => rsx! {
                                    div { class: "status-label cancelled", style: "margin-top: 8px;", "🚫 已取消执行" }
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
