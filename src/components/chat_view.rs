use crate::models::{ActionStatus, ChatMessage};
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use dioxus::{document::eval, prelude::*};
use serde::Deserialize;

#[derive(PartialEq)]
enum TextSegment {
    Text(String),
    Code(String),
}

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

// 🔥 新增：解析函数，将混合文本切分为 普通文本 和 代码块
fn parse_markdown_segments(text: &str) -> Vec<TextSegment> {
    let mut segments = Vec::new();
    let parts = text.split("```");

    // 简单的偶数位置是文本，奇数位置是代码（假设代码块总是成对出现）
    // 这是一个简化的解析，更健壮的方式是使用 pulldown-cmark 库
    for (i, part) in parts.enumerate() {
        if part.trim().is_empty() {
            continue;
        }

        if i % 2 == 0 {
            segments.push(TextSegment::Text(part.to_string()));
        } else {
            // 去掉可能存在的 "python" 前缀
            let code_content = if part.trim_start().starts_with("python") {
                part.replacen("python", "", 1)
            } else {
                part.to_string()
            };
            segments.push(TextSegment::Code(code_content.trim().to_string()));
        }
    }
    segments
}

// 🔥 新增：辅助函数，简单处理行内的 **加粗** 语法
// 这样 "🚨 **检测到...**" 里的文字就会变成 <strong />，配合 CSS 变深红色
fn render_markdown_inline(text: &str) -> Element {
    let parts: Vec<&str> = text.split("**").collect();
    rsx! {
        {
            parts
                .iter()
                .enumerate()
                .map(|(i, part)| {
                    if i % 2 == 1 {
                        rsx! {
                            strong { "{part}" }
                        }
                    } else {
                        rsx! {
                            span { "{part}" }
                        }
                    }
                })
        }
    }
}

// 将复杂的文本段落渲染逻辑提取为独立函数
// 这避免了在 rsx! 或 map 闭包内部写复杂的 let 语句导致的解析错误
fn render_text_segment_content(text: String, is_undone: bool) -> Element {
    let mut elements = Vec::new();
    let mut current_quote_lines = Vec::new();

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('>') {
            let content = trimmed.strip_prefix('>').unwrap_or("").trim();
            current_quote_lines.push(content);
        } else {
            // 如果之前有引用块，先渲染并清空
            if !current_quote_lines.is_empty() {
                let quote_text = current_quote_lines.join("\n");
                elements.push(rsx! {
                    blockquote { {render_markdown_inline(&quote_text)} }
                });
                current_quote_lines.clear();
            }
            // 渲染普通文本行
            if !trimmed.is_empty() {
                elements.push(rsx! {
                    div { style: "min-height: 1.2em;", {render_markdown_inline(line)} }
                });
            }
        }
    }

    // 处理结尾残留的引用块
    if !current_quote_lines.is_empty() {
        let quote_text = current_quote_lines.join("\n");
        elements.push(rsx! {
            blockquote { {render_markdown_inline(&quote_text)} }
        });
    }

    rsx! {
        div { style: if is_undone { "white-space: pre-wrap; margin-bottom: 8px; text-decoration: line-through; opacity: 0.7;" } else { "white-space: pre-wrap; margin-bottom: 8px;" },
            {elements.into_iter()}
        }
    }
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
        let bubble_class = if is_undone { "bubble undone-state" } else { "bubble" };

        // 如果是用户发送的消息，先尝试用 format_user_message 解析 JSON 并美化。
        // 如果是 AI 回复的消息，则保持原样。
        let display_text = if msg.is_user {
            format_user_message(&msg.text)
        } else {
            msg.text.clone()
        };

        // 解析文本段落
        let segments = parse_markdown_segments(&display_text);

        // 构建内容元素
        let content_elements = segments.into_iter().map(|seg| {
            match seg {
               TextSegment::Text(t) => render_text_segment_content(t, is_undone),
                TextSegment::Code(c) => rsx! {
                    // 🔥 渲染为 Highlight.js 可识别的结构
                    div { style: "margin-bottom: 10px;",
                        pre {
                            // 这里 class="language-python" 必须要有，hljs 靠这个识别
                            code { class: "language-python", "{c}" }
                        }
                    }
                }
            }
        });

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
            // 报错状态下显示重试按钮，防止死胡同
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
                    {content_elements}

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
                                    // 这里也是代码，也加上高亮
                                    pre {
                                        code { class: "language-python", "{code}" }
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
