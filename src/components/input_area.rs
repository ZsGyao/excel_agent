use crate::models::{AppConfig, ChatMessage};
use crate::services::{ai, python};
use dioxus::prelude::*;

#[component]
pub fn InputArea(
    messages: Signal<Vec<ChatMessage>>,
    last_file_path: Signal<String>,
    is_loading: Signal<bool>,
    config: Signal<AppConfig>,
) -> Element {
    let mut input_text = use_signal(|| String::new());

    let mut handle_send = move || {
        if input_text.read().is_empty() {
            return;
        }

        let user_prompt = input_text.read().clone();
        let file_path = last_file_path.read().clone();

        // 1. 获取当前激活的配置
        let cfg = config.read();
        let active_profile = cfg
            .active_profile_id
            .as_ref()
            .and_then(|id| cfg.profiles.iter().find(|p| &p.id == id));

        // 校验配置是否有效
        let (key, url, model) = match active_profile {
            Some(p) if !p.api_key.is_empty() => {
                (p.api_key.clone(), p.base_url.clone(), p.model_id.clone())
            }
            _ => {
                let err_id = messages.read().len();
                messages.write().push(ChatMessage {
                    id: err_id,
                    text: "❌ 请先在设置中配置并选中一个有效的模型（API Key 不能为空）！".into(),
                    is_user: false,
                });
                return;
            }
        };

        // ... UI 更新逻辑不变 ...
        let new_id = messages.read().len();
        messages.write().push(ChatMessage {
            id: new_id,
            text: user_prompt.clone(),
            is_user: true,
        });
        input_text.set(String::new());

        if file_path.is_empty() {
            let err_id = messages.read().len();
            messages.write().push(ChatMessage {
                id: err_id,
                text: "请先拖入一个 Excel 文件！".into(),
                is_user: false,
            });
            return;
        }

        is_loading.set(true);

        spawn(async move {
            let columns = python::get_excel_columns(&file_path);
            let ai_result = ai::call_ai(key, url, model, user_prompt, columns).await;

            // ... 处理结果逻辑不变 ...
            match ai_result {
                Ok(code) => {
                    let exec_result = python::run_python_code(&file_path, &code);
                    let final_reply =
                        format!("🔧 执行代码:\n{}\n\n📊 结果:\n{}", code, exec_result);
                    let ai_id = messages.read().len();
                    messages.write().push(ChatMessage {
                        id: ai_id,
                        text: final_reply,
                        is_user: false,
                    });
                }
                Err(err) => {
                    let err_id = messages.read().len();
                    messages.write().push(ChatMessage {
                        id: err_id,
                        text: format!("❌ 失败: {}", err),
                        is_user: false,
                    });
                }
            }
            is_loading.set(false);
        });
    };

    rsx! {
        div { class: "input-section",
            input {
                placeholder: "输入需求...",
                value: "{input_text}",
                oninput: move |evt| input_text.set(evt.value()),
                disabled: is_loading(),
                onkeydown: move |evt| { if evt.key() == Key::Enter { handle_send(); } }
            },
            button {
                class: "btn-send",
                onclick: move |_| handle_send(),
                "发送"
            }
        }
    }
}
