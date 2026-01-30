use crate::models::{AppConfig, ChatMessage, PyExecResult};
use crate::services::{ai, python};
use dioxus::prelude::*;
use tokio::task;
use uuid::Uuid;

#[component]
pub fn InputArea(
    messages: Signal<Vec<ChatMessage>>,
    last_file_path: Signal<String>,
    is_loading: Signal<bool>,
    config: Signal<AppConfig>,
) -> Element {
    let mut input_text = use_signal(|| String::new());

    let mut handle_send = move || {
        if input_text.read().trim().is_empty() {
            return;
        }

        let user_prompt = input_text.read().clone();
        let file_path = last_file_path.read().clone();

        let new_id = messages.read().len();
        messages.write().push(ChatMessage {
            id: new_id,
            text: user_prompt.clone(),
            is_user: true,
            table: None,
            temp_id: None,
            status: crate::models::ActionStatus::None,
            image: None,
        });
        input_text.set(String::new());

        // Start async task
        is_loading.set(true);

        spawn(async move {
            // Basic check
            if file_path.is_empty() {
                let err_id = messages.read().len();
                messages.write().push(ChatMessage {
                    id: err_id,
                    text: "⚠️ 请先拖入一个 Excel 文件（哪怕是空文件），我才能开始工作。".into(),
                    is_user: false,
                    table: None,
                    temp_id: None,
                    status: crate::models::ActionStatus::None,
                    image: None,
                });
                is_loading.set(false);
                return;
            }

            // Read config
            let (key, url, model) = {
                let cfg = config.read();
                let active_profile = cfg
                    .active_profile_id
                    .as_ref()
                    .and_then(|id| cfg.profiles.iter().find(|p| &p.id == id));

                match active_profile {
                    Some(p) if !p.api_key.is_empty() => {
                        (p.api_key.clone(), p.base_url.clone(), p.model_id.clone())
                    }
                    _ => {
                        let err_id = messages.read().len();
                        messages.write().push(ChatMessage {
                            id: err_id,
                            text: "❌ 配置错误：请在设置中选中一个模型，并确保 API Key 不为空。"
                                .into(),
                            is_user: false,
                            table: None,
                            temp_id: None,
                            status: crate::models::ActionStatus::None,
                            image: None,
                        });
                        is_loading.set(false);
                        return;
                    }
                }
            };

            // Prepare backend task
            let file_path_clone = file_path.clone();
            let columns_result =
                task::spawn_blocking(move || python::get_excel_columns(&file_path_clone)).await;

            let columns = match columns_result {
                Ok(cols) => cols,
                Err(_) => {
                    let err_id = messages.read().len();
                    messages.write().push(ChatMessage {
                        id: err_id,
                        text: "❌ 系统错误: 线程崩溃".into(),
                        is_user: false,
                        table: None,
                        temp_id: None,
                        status: crate::models::ActionStatus::None,
                        image: None,
                    });
                    is_loading.set(false);
                    return;
                }
            };

            /* Auto fix complie error.. loop */
            // Max retry times
            const MAX_RETRIES: usize = 3;
            // Current prompt, init prompt is user input
            let mut current_prompt = user_prompt.clone();
            // Is success
            let mut success = false;

            for attempt in 0..MAX_RETRIES {
                let ai_result = ai::call_ai(
                    key.clone(),
                    url.clone(),
                    model.clone(),
                    current_prompt.clone(),
                    columns.clone(),
                )
                .await;

                match ai_result {
                    Ok(reply) => {
                        if reply.reply_type == "code" {
                            let file_path_for_exec = file_path.clone();
                            let code_for_exec = reply.content.clone();

                            // Gnerate uuid
                            let operation_id = Uuid::new_v4().to_string();
                            let op_id_for_exec = operation_id.clone();

                            // Execute Python Backend
                            let exec_join = task::spawn_blocking(move || {
                                python::run_python_code(
                                    &file_path_for_exec,
                                    &code_for_exec,
                                    &op_id_for_exec,
                                )
                            })
                            .await;

                            match exec_join {
                                Ok(json_str) => {
                                    // Prase the python return JSON
                                    match serde_json::from_str::<PyExecResult>(&json_str) {
                                        Ok(res) => {
                                            if res.status == "error" {
                                                // Error, try again
                                                println!(
                                                    "尝试 #{} 失败: {}",
                                                    attempt + 1,
                                                    res.message
                                                );
                                                current_prompt = format!(
                                                                "你生成的代码运行报错了。\n\n刚才的代码:\n{}\n\n报错信息:\n{}\n\n请分析错误原因，并重新生成修正后的完整代码。",
                                                                reply.content,
                                                                res.message
                                                            );

                                                if attempt == MAX_RETRIES - 1 {
                                                    let err_id = messages.read().len();
                                                    messages.write().push(ChatMessage {
                                                        id: err_id,
                                                        text: format!(
                                                            "🤯 自动修复失败。\n最后报错:\n{}",
                                                            res.message
                                                        ),
                                                        is_user: false,
                                                        table: None,
                                                        temp_id: None,
                                                        status: crate::models::ActionStatus::None,
                                                        image: None,
                                                    });
                                                }
                                            } else {
                                                // Success, show result and table
                                                let final_reply = format!(
                                                    "🔧 执行代码:\n{}\n\n{}",
                                                    reply.content, res.message
                                                );
                                                let ai_id = messages.read().len();

                                                messages.write().push(ChatMessage {
                                                    id: ai_id,
                                                    text: final_reply,
                                                    is_user: false,
                                                    table: res.preview,
                                                    temp_id: Some(operation_id.clone()),
                                                    status: crate::models::ActionStatus::Pending,
                                                    image: None,
                                                });
                                                break;
                                            }
                                        }
                                        Err(e) => {
                                            // Prase JSON Failed
                                            let err_id = messages.read().len();
                                            messages.write().push(ChatMessage {
                                                id: err_id,
                                                text: format!("❌ 内部通讯错误: {}", e),
                                                is_user: false,
                                                table: None,
                                                temp_id: None,
                                                status: crate::models::ActionStatus::None,
                                                image: None,
                                            });
                                            break;
                                        }
                                    }
                                }
                                Err(_) => {
                                    let err_id = messages.read().len();
                                    messages.write().push(ChatMessage {
                                        id: err_id,
                                        text: "❌ Python 线程崩溃".into(),
                                        is_user: false,
                                        table: None,
                                        temp_id: None,
                                        status: crate::models::ActionStatus::None,
                                        image: None,
                                    });
                                    break;
                                }
                            }
                        } else {
                            // Chat
                            let ai_id = messages.read().len();
                            messages.write().push(ChatMessage {
                                id: ai_id,
                                text: reply.content,
                                is_user: false,
                                table: None,
                                temp_id: None,
                                status: crate::models::ActionStatus::None,
                                image: None,
                            });
                            break;
                        }
                    }
                    Err(err) => {
                        let err_id = messages.read().len();
                        messages.write().push(ChatMessage {
                            id: err_id,
                            text: format!("❌ 网络请求失败: {}", err),
                            is_user: false,
                            table: None,
                            temp_id: None,
                            status: crate::models::ActionStatus::None,
                            image: None,
                        });
                        break;
                    }
                }
            }

            is_loading.set(false);
        });
    };

    rsx! {
        div { class: "input-container",
            input {
                class: "chat-input",
                placeholder: "输入需求...",
                value: "{input_text}",
                oninput: move |evt| input_text.set(evt.value()),
                disabled: is_loading(),
                onkeydown: move |evt| {
                    if evt.key() == Key::Enter && !evt.modifiers().contains(Modifiers::SHIFT) {
                        evt.prevent_default();
                        handle_send();
                    }
                },
            }
            button { class: "send-btn", onclick: move |_| handle_send(), "发送" }
        }
    }
}
