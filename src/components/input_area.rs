use crate::models::{ActionStatus, AppConfig, ChatMessage};
use crate::services::config::save_config;
use dioxus::prelude::*;

#[component]
pub fn InputArea(
    messages: Signal<Vec<ChatMessage>>,
    active_files: Signal<Vec<String>>,
    is_loading: Signal<bool>,
    config: Signal<AppConfig>,
    error_fix_signal: Signal<Option<String>>,
    on_run_code: EventHandler<usize>,
    on_open_file: EventHandler<()>,
) -> Element {
    let mut input_text = use_signal(|| String::new());

    // 🌟 瘦身后的核心逻辑：只发通知，不干重活！
    let mut perform_request = move |prompt_text: String, is_auto_fix: bool| {
        if is_loading() {
            return;
        }
        is_loading.set(true);

        let user_id = messages.read().len();
        let display = if is_auto_fix {
            format!("自动修复: {}", prompt_text)
        } else {
            prompt_text
        };

        // 1. 压入用户消息
        messages
            .write()
            .push(ChatMessage::new(user_id, &display, true));

        // 2. 压入 AI 占位消息 (状态直接设为 Running)
        let ai_id = messages.read().len();
        let mut ai_msg = ChatMessage::loading(ai_id);
        ai_msg.status = ActionStatus::Running;
        messages.write().push(ai_msg);

        // 3. 🚀 呼叫 Controller 全盘接管！(它会去嗅探、调AI、执行代码)
        on_run_code.call(ai_id);

        // 释放加载状态
        is_loading.set(false);
    };

    // 监听错误信号，触发自动修复
    use_effect(move || {
        if let Some(err) = error_fix_signal() {
            let err_clone = err.clone();
            spawn(async move {
                error_fix_signal.set(None);
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

        let current_idx = profiles
            .iter()
            .position(|p| Some(&p.id) == cfg.active_profile_id.as_ref())
            .unwrap_or(0);
        let next_idx = (current_idx + 1) % profiles.len();
        cfg.active_profile_id = Some(profiles[next_idx].id.clone());

        config.set(cfg.clone());
        save_config(&cfg);
    };

    let active_model_name = config.read().active_profile().name.clone();

    // UI 部分原封不动保留你的设计
    rsx! {
        div { class: "input-container",
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
                    onclick: move |_| on_open_file.call(()),
                    "📎"
                }
            }

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
