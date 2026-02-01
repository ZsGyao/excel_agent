use crate::models::{ActionStatus, AppConfig, ChatMessage};
use crate::services::ai;
use crate::services::python::get_excel_info;
use dioxus::prelude::*;

fn extract_python_code(text: &str) -> Option<String> {
    let start_marker = "```python";
    let end_marker = "```";
    if let Some(start) = text.find(start_marker) {
        let code_start = start + start_marker.len();
        if let Some(end) = text[code_start..].find(end_marker) {
            return Some(text[code_start..code_start + end].trim().to_string());
        }
    }
    if let Some(start) = text.find("```") {
        let code_start = start + 3;
        if let Some(end) = text[code_start..].find("```") {
            let code = text[code_start..code_start + end].trim();
            if !code.is_empty() && (code.contains("import") || code.contains("print")) {
                return Some(code.to_string());
            }
        }
    }
    None
}

#[component]
pub fn InputArea(
    messages: Signal<Vec<ChatMessage>>,
    last_file_path: Signal<String>,
    is_loading: Signal<bool>,
    config: Signal<AppConfig>,
    // 🔥 信号桥：接收错误信息
    error_fix_signal: Signal<Option<String>>,
    // 🔥 回调：请求立即运行 (用于自动修复)
    on_run_code: EventHandler<usize>,
) -> Element {
    let mut input_text = use_signal(|| String::new());

    // 核心请求逻辑
    let mut perform_request = move |prompt_text: String, is_auto_fix: bool| {
        if is_loading() {
            return;
        }
        is_loading.set(true);

        let user_id = messages.read().len();
        let display = if is_auto_fix {
            "🤖 自动修复: 正在修正代码..."
        } else {
            &prompt_text
        };
        messages
            .write()
            .push(ChatMessage::new(user_id, display, true));

        let ai_id = messages.read().len();
        messages.write().push(ChatMessage::loading(ai_id));

        let file = last_file_path();

        spawn(async move {
            let cfg = config.read().clone();

            // 构建上下文
            let ctx = if !file.is_empty() {
                let info = get_excel_info(&file).await;
                Some(format!("Target: r\"{}\"\nInfo: {}", file, info))
            } else {
                None
            };

            // 如果是修复，修改 Prompt
            let prompt = if is_auto_fix {
                format!(
                    "Previous code failed:\n{}\n\nFix it and output full code.",
                    prompt_text
                )
            } else {
                prompt_text
            };

            match ai::call_ai(&cfg, &prompt, ctx).await {
                Ok(content) => {
                    let mut msgs = messages.write();
                    if let Some(code) = extract_python_code(&content) {
                        // === 是代码 ===
                        let clean_text = content.replace("```python", "").replace("```", "");
                        msgs[ai_id].text = clean_text;
                        msgs[ai_id].pending_code = Some(code);

                        if is_auto_fix {
                            // 自动修复模式：直接运行，不需用户确认
                            msgs[ai_id].status = ActionStatus::Running;
                            drop(msgs); // 释放锁
                            on_run_code.call(ai_id); // 🚀 立即触发运行
                        } else {
                            // 正常模式：等待确认
                            msgs[ai_id].status = ActionStatus::WaitingConfirmation;
                        }
                    } else {
                        // === 是闲聊 ===
                        msgs[ai_id].text = content;
                        msgs[ai_id].status = ActionStatus::Success;
                    }
                }
                Err(e) => {
                    let mut msgs = messages.write();
                    msgs[ai_id].text = format!("Err: {}", e);
                    msgs[ai_id].status = ActionStatus::Error(e.to_string());
                }
            }
            is_loading.set(false);
        });
    };

    // 🔥 监听错误信号，触发自动修复
    use_effect(move || {
        if let Some(err) = error_fix_signal() {
            let err_clone = err.clone();
            spawn(async move {
                // 重置信号防止循环
                error_fix_signal.set(None);
                // 发起修复请求
                perform_request(err_clone, true);
            });
        }
    });

    let mut handle_send = move |_| {
        let text = input_text();
        if text.trim().is_empty() {
            return;
        }
        input_text.set(String::new());
        perform_request(text, false);
    };

    rsx! {
        div { class: "input-container",
            textarea {
                class: "chat-input",
                value: "{input_text}",
                oninput: move |evt| input_text.set(evt.value()),
                onkeydown: move |evt| {
                    if evt.key() == Key::Enter && !evt.modifiers().contains(Modifiers::SHIFT) {
                        handle_send(());
                    }
                },
            }
            button {
                class: "send-btn",
                disabled: is_loading(),
                onclick: move |_| handle_send(()),
                if is_loading() {
                    "..."
                } else {
                    "发送"
                }
            }
        }
    }
}
