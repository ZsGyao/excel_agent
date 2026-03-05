use crate::models::{ActionStatus, ChatMessage};
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use dioxus::{document::eval, prelude::*};
use pulldown_cmark::{html, Options, Parser};
use serde::Deserialize;

#[derive(Deserialize)]
struct ChatPayload {
    raw_query: String,
    mentions: Vec<MentionData>,
}

#[derive(Deserialize)]
struct MentionData {
    placeholder: String,
    file: String,
    sheet: String,
    col: String,
}

/// 工业级 Markdown 转 HTML 引擎
fn render_markdown_to_html(md_text: &str) -> String {
    // 开启扩展功能：支持表格、删除线、任务列表
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);

    let parser = Parser::new_ext(md_text, options);
    let mut html_output = String::new();

    // 一键将 Markdown 转换为标准 HTML
    html::push_html(&mut html_output, parser);

    html_output
}

/// 专门用于将后端的 JSON Payload 转化为漂亮的 UI 文本
pub fn format_user_message(raw_msg: &str) -> String {
    // 尝试解析是否为带有胶囊的 JSON 协议
    if let Ok(payload) = serde_json::from_str::<ChatPayload>(raw_msg) {
        let mut display_text = payload.raw_query;

        for mention in payload.mentions {
            let file = String::from_utf8(B64.decode(&mention.file).unwrap_or_default())
                .unwrap_or_default();
            let sheet = String::from_utf8(B64.decode(&mention.sheet).unwrap_or_default())
                .unwrap_or_default();
            let col =
                String::from_utf8(B64.decode(&mention.col).unwrap_or_default()).unwrap_or_default();

            let display_name = if !col.is_empty() {
                // 取出最后的列名
                let short_col = col.split("@|||@").last().unwrap_or(&col);
                format!("`🏷️ {}`", short_col) // 用 markdown 的反引号包裹，渲染出类似胶囊的背景
            } else if !sheet.is_empty() {
                format!("`📑 {}`", sheet)
            } else if !file.is_empty() {
                // 取出纯文件名
                let short_file = file.replace("\\", "/");
                let file_name = short_file.split('/').last().unwrap_or(&file);
                format!("`📄 {}`", file_name)
            } else {
                "".to_string()
            };

            // 把 {{REF_0}} 替换成漂亮的名字
            display_text = display_text.replace(&mention.placeholder, &display_name);
        }
        return display_text;
    }

    // 如果是普通文本聊天（不是 JSON），直接原样返回
    raw_msg.to_string()
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

        // 触发 Highlight.js 对页面上所有代码块进行高亮
        let _ = eval(
            r#"
            setTimeout(() => {
                if (window.hljs) {
                    window.hljs.highlightAll();
                }
            }, 100); 
        "#,
        );
    });

    let msgs = messages.read().clone();

    // 预渲染
    let rendered_msgs = msgs.iter().map(|msg| {
        let msg_id = msg.id;
        let has_code = msg.pending_code.is_some();
        let is_error = matches!(msg.status, ActionStatus::Error(_));
        let is_undone = matches!(msg.status, ActionStatus::Undone);

        // 如果是用户发送的消息，先尝试用 format_user_message 解析 JSON 并美化。
        // 如果是 AI 回复的消息，则保持原样。
        let display_text = if msg.is_user {
            format_user_message(&msg.text)
        } else {
            msg.text.clone()
        };

        // 转换成带原生 <table>, <pre>, <blockquote> 标签的 HTML！
        let html_content = render_markdown_to_html(&display_text);

        // 底部交互栏逻辑
        let status_badge = match msg.status {
            ActionStatus::Running => rsx! {
                div { class: "status-badge running",
                    div { class: "spinner" }
                    "AI 正在处理数据..."
                }
            },
            ActionStatus::Success => rsx! {
                div { class: "status-action-row",
                    div { class: "status-badge success", "✅ 操作已完成" }
                    if msg.backup_paths.is_some() && !is_undone {
                        button {
                            class: "action-btn undo-btn",
                            onclick: move |_| on_undo.call(msg_id),
                            "↩️ 撤销"
                        }
                    }
                }
            },
            ActionStatus::Error(ref e) => rsx! {
                div { class: "status-badge error",
                    div { class: "error-title", "❌ 执行遇到问题" }
                    div { class: "error-detail", "{e}" }
                }
            },
            ActionStatus::WaitingConfirmation => rsx! {
                div { class: "confirm-panel",
                    div { class: "confirm-title", "⚡ 准备就绪，是否执行？" }
                    div { class: "confirm-actions",
                        button {
                            class: "action-btn run-btn",
                            onclick: move |_| on_confirm.call(msg_id),
                            "✅ 执行"
                        }
                        button {
                            class: "action-btn cancel-btn",
                            onclick: move |_| on_cancel.call(msg_id),
                            "🚫 取消"
                        }
                    }
                }
            },
            ActionStatus::Undone => rsx! {
                div { class: "status-badge undone", "↩️ 数据已恢复至操作前状态" }
            },
            _ => rsx! {}
        };

        rsx! {
            div {
                key: "{msg_id}",
                class: if msg.is_user { "chat-row row-user" } else { "chat-row row-ai" },

                div { class: if msg.is_user { "chat-bubble bubble-user" } else { "chat-bubble bubble-ai" },
                    // 主文本内容
                    div {
                        class: if is_undone { "bubble-content content-undone" } else { "bubble-content" },
                        dangerous_inner_html: "{html_content}",
                    }

                    // 状态与操作栏
                    if !msg.is_user {
                        div { class: "bubble-footer",
                            {status_badge}

                            // 优雅的代码折叠面板
                            if has_code || is_error {
                                details { class: "sleek-details", open: "false",
                                    summary { "⚙️ 查看执行代码" }
                                    div { class: "details-content",
                                        if let Some(code) = &msg.pending_code {
                                            div { class: "code-block-wrapper",
                                                pre {
                                                    code { class: "language-python",
                                                        "{code}"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    });

    rsx! {
        div { id: "chat-scroll", class: "chat-scroll", {rendered_msgs} }
    }
}
