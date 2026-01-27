use crate::models::{AppConfig, ChatMessage};
use crate::services::{ai, python};
use dioxus::prelude::*;
use tokio::task;

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

        // 1. UI 瞬间响应
        let new_id = messages.read().len();
        messages.write().push(ChatMessage {
            id: new_id,
            text: user_prompt.clone(),
            is_user: true,
        });
        input_text.set(String::new());

        // 2. 开启异步任务
        is_loading.set(true);

        spawn(async move {
            // A. 基础校验
            if file_path.is_empty() {
                let err_id = messages.read().len();
                messages.write().push(ChatMessage {
                    id: err_id,
                    text: "⚠️ 请先拖入一个 Excel 文件（哪怕是空文件），我才能开始工作。".into(),
                    is_user: false,
                });
                is_loading.set(false);
                return;
            }

            // B. 读取配置
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
                        });
                        is_loading.set(false);
                        return;
                    }
                }
            };

            // C. 准备后台执行环境
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
                    });
                    is_loading.set(false);
                    return;
                }
            };

            // ======================================================
            // 🔥 核心升级：自动修复循环 (Auto-Fix Loop)
            // ======================================================

            // 最大重试次数
            const MAX_RETRIES: usize = 3;
            // 当前的提示词 (初始为用户的输入)
            let mut current_prompt = user_prompt.clone();
            // 记录是否成功
            let mut success = false;

            for attempt in 0..MAX_RETRIES {
                // 1. 调用 AI
                // 注意：这里我们每次都用 updated prompt 重新请求
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
                            // 2. 后台执行代码
                            let file_path_for_exec = file_path.clone();
                            let code_for_exec = reply.content.clone();

                            let exec_join = task::spawn_blocking(move || {
                                python::run_python_code(&file_path_for_exec, &code_for_exec)
                            })
                            .await;

                            match exec_join {
                                Ok(exec_result) => {
                                    // 3. 检查 Python 执行结果是否包含错误标记
                                    // 我们在 backend.py 里定义了：如果出错，返回字符串以 "❌" 开头
                                    if exec_result.trim().starts_with("❌") {
                                        println!("尝试 #{} 失败: {}", attempt + 1, exec_result);

                                        // ⚠️ 遇到错误！不要显示给用户，而是准备下一轮的 Prompt
                                        // 构造“修复提示词”
                                        current_prompt = format!(
                                            "你生成的代码运行报错了。\n\n刚才的代码:\n{}\n\n报错信息:\n{}\n\n请分析错误原因，并重新生成修正后的完整代码。",
                                            reply.content,
                                            exec_result
                                        );

                                        // 如果是最后一次尝试，依然失败，那就没办法了，显示错误
                                        if attempt == MAX_RETRIES - 1 {
                                            let err_id = messages.read().len();
                                            let error_msg = format!("🤯 我尽力了，尝试自动修复 {MAX_RETRIES} 次均失败。\n最后一次报错:\n{}", exec_result);
                                            messages.write().push(ChatMessage {
                                                id: err_id,
                                                text: error_msg,
                                                is_user: false,
                                            });
                                        }
                                        // 继续下一次循环 (continue)
                                    } else {
                                        // ✅ 成功！显示结果并跳出循环
                                        let final_reply = format!(
                                            "🔧 执行代码:\n{}\n\n📊 结果:\n{}",
                                            reply.content, exec_result
                                        );
                                        let ai_id = messages.read().len();
                                        messages.write().push(ChatMessage {
                                            id: ai_id,
                                            text: final_reply,
                                            is_user: false,
                                        });
                                        success = true;
                                        break; // 退出重试循环
                                    }
                                }
                                Err(_) => {
                                    // 线程崩溃这种严重错误直接报错
                                    let err_id = messages.read().len();
                                    messages.write().push(ChatMessage {
                                        id: err_id,
                                        text: "❌ Python 线程崩溃".into(),
                                        is_user: false,
                                    });
                                    break;
                                }
                            }
                        } else {
                            // 如果 AI 返回的是闲聊，直接显示并结束
                            let ai_id = messages.read().len();
                            messages.write().push(ChatMessage {
                                id: ai_id,
                                text: reply.content,
                                is_user: false,
                            });
                            success = true;
                            break;
                        }
                    }
                    Err(err) => {
                        // 网络请求本身失败，重试也没用（通常是 Key 错或者断网），直接报错
                        let err_id = messages.read().len();
                        messages.write().push(ChatMessage {
                            id: err_id,
                            text: format!("❌ 网络请求失败: {}", err),
                            is_user: false,
                        });
                        break;
                    }
                }
            } // end loop

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
