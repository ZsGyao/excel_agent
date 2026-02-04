use crate::models::{ActionStatus, AppConfig, ChatMessage};
use crate::services::ai;
use crate::services::config::save_config;
use crate::services::python::get_multi_file_summary;
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

// 从文本中移除代码块，只保留对话文字
fn remove_code_block(text: &str) -> String {
    if let Some(start) = text.find("```") {
        if let Some(end) = text[start + 3..].find("```") {
            let end_pos = start + 3 + end + 3;
            let before = &text[..start];
            let after = &text[end_pos..];
            return format!("{}{}", before, after).trim().to_string();
        }
    }
    text.to_string()
}

#[component]
pub fn InputArea(
    messages: Signal<Vec<ChatMessage>>,
    active_files: Signal<Vec<String>>,
    is_loading: Signal<bool>,
    config: Signal<AppConfig>,
    // 信号桥：接收错误信息
    error_fix_signal: Signal<Option<String>>,
    // 回调：请求立即运行 (用于自动修复)
    on_run_code: EventHandler<usize>,
    on_open_file: EventHandler<()>,
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
            "自动修复: 正在修正代码..."
        } else {
            &prompt_text
        };
        messages
            .write()
            .push(ChatMessage::new(user_id, display, true));

        let ai_id = messages.read().len();
        messages.write().push(ChatMessage::loading(ai_id));

        let files = active_files.read().clone();

        spawn(async move {
            let cfg = config.read().clone();

            // 构建上下文
            let context_data = if !files.is_empty() {
                let summary = get_multi_file_summary(files.clone()).await;
                Some(format!(
                    "Target File Path: r\"{:?}\"\n\nData Context (First 5 rows):\n{}",
                    files, summary
                ))
            } else {
                None
            };

            // 构造最终 Prompt
            let final_prompt = if is_auto_fix {
                // 如果是修复，把上下文也带上，防止 AI 忘了数据长啥样
                format!("Previous code failed.\nContext:\n{:?}\n\nUser Request: {}\n\nFix the code based on the context.", context_data, prompt_text)
            } else {
                prompt_text
            };

            // 调用 AI (注意：这里把 context_data 传进去，ai::call_ai 内部会拼接到 System Prompt 里)
            match ai::call_ai(&cfg, &final_prompt, context_data).await {
                Ok(content) => {
                    let mut msgs = messages.write();
                    if let Some(code) = extract_python_code(&content) {
                        // === 是代码 ===
                        let clean_text = remove_code_block(&content);
                        // 如果移除后为空，给一个默认提示
                        msgs[ai_id].text = if clean_text.is_empty() {
                            "已生成操作代码，请确认执行：".to_string()
                        } else {
                            clean_text
                        };

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

    // 切换模型逻辑
    let mut switch_model = move || {
        let mut cfg = config.read().clone();
        let profiles = &cfg.profiles;
        if profiles.is_empty() {
            return;
        }

        // 找到当前模型索引，切换到下一个
        let current_idx = profiles
            .iter()
            .position(|p| Some(&p.id) == cfg.active_profile_id.as_ref())
            .unwrap_or(0);
        let next_idx = (current_idx + 1) % profiles.len();
        cfg.active_profile_id = Some(profiles[next_idx].id.clone());

        config.set(cfg.clone());
        save_config(&cfg); // 持久化保存
    };

    let active_model_name = config.read().active_profile().name.clone();

    // button {
    //                                     class: "confirm-btn", // 复用现有按钮样式
    //                                     style: "font-size: 16px; padding: 10px 24px;",
    //                                     onclick: open_file_dialog,
    //                                     "📂 打开本地 Excel 文件"
    //                                 }

    rsx! {
        // div 的 class 已经在 main.rs 的容器中被控制了 (center-mode vs chat-mode)
        div { class: "input-container",
            // 🔥 1. 上方工具栏：模型选择
            div { class: "input-toolbar",
                div {
                    class: "model-selector",
                    onclick: move |_| switch_model(),
                    title: "点击切换模型",
                    "{active_model_name} ▾"
                }
                button {
                    class: "tool-btn",
                    title: "添加文件",
                    // 🔥 绑定到从 main.rs 传进来的回调
                    onclick: move |_| on_open_file.call(()),
                    "📎"
                }
            }

            // 🔥 2. 下方输入框 + 按钮
            div { class: "input-wrapper",
                textarea {
                    class: "chat-input",
                    placeholder: "输入指令，例如：把 A1 标红...",
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
                        "⬆"
                    }
                }
            }
        }
    }
}
