#![allow(non_snake_case)]

use dioxus::desktop::{Config, LogicalSize, WindowBuilder};
use dioxus::html::HasFileData;
use dioxus::prelude::*;
use pyo3::prelude::*;
use pyo3::types::PyModule;
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;

// --- 1. API 数据结构定义 ---
#[derive(Serialize, Deserialize, Clone, Debug)]
struct ChatRequest {
    model: String,
    messages: Vec<MessageApi>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct MessageApi {
    role: String,
    content: String,
}

#[derive(Deserialize, Clone, Debug)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize, Clone, Debug)]
struct Choice {
    message: MessageApi,
}

// --- 2. 视图状态定义 ---
#[derive(Clone, PartialEq, Copy)]
enum View {
    Chat,
    Settings,
}

#[derive(Clone, PartialEq)]
struct ChatMessage {
    id: usize,
    text: String,
    is_user: bool,
}

fn main() {
    dioxus_logger::init(tracing::Level::INFO).expect("failed to init logger");

    // --- Python 环境配置 ---
    let current_dir = env::current_dir().unwrap();
    let py_env_path = current_dir.join("py_env");

    let std_lib = py_env_path.join("python311.zip");
    let site_packages = py_env_path.join("Lib\\site-packages");
    let dlls = py_env_path.join("DLLs");

    let new_python_path = format!(
        "{};{};{}",
        std_lib.display(),
        site_packages.display(),
        dlls.display()
    );

    env::set_var("PYTHONHOME", &py_env_path);
    env::set_var("PYTHONPATH", &new_python_path);
    // -----------------------

    let config = Config::new()
        .with_custom_head(r#"<link rel="stylesheet" href="style.css">"#.to_string())
        .with_window(
            WindowBuilder::new()
                .with_title("Excel AI Agent")
                .with_inner_size(LogicalSize::new(900.0, 700.0))
                .with_resizable(true),
        );

    LaunchBuilder::desktop().with_cfg(config).launch(App);
}

// --- 3. 后端功能函数 ---

fn get_excel_columns(file_path: &str) -> String {
    let current_dir = env::current_dir().unwrap();
    let backend_path = current_dir.join("backend.py");
    let backend_code = fs::read_to_string(backend_path).unwrap_or_default();

    Python::with_gil(|py| -> PyResult<String> {
        let backend = PyModule::from_code(py, &backend_code, "backend.py", "backend_module")?;
        let columns: String = backend
            .getattr("get_columns")?
            .call1((file_path,))?
            .extract()?;
        Ok(columns)
    })
    .unwrap_or_else(|e| format!("读取列名失败: {}", e))
}

fn run_python_code(file_path: &str, code: &str) -> String {
    let current_dir = env::current_dir().unwrap();
    let backend_path = current_dir.join("backend.py");
    let backend_code = fs::read_to_string(backend_path).unwrap_or_default();

    Python::with_gil(|py| -> PyResult<String> {
        let backend = PyModule::from_code(py, &backend_code, "backend.py", "backend_module")?;
        let result: String = backend
            .getattr("execute_analysis")?
            .call1((file_path, code))?
            .extract()?;
        Ok(result)
    })
    .unwrap_or_else(|e| format!("Python 执行出错: {}", e))
}

async fn call_ai(
    api_key: String,
    base_url: String,
    model_name: String,
    prompt: String,
    columns: String,
) -> Result<String, String> {
    if api_key.is_empty() {
        return Err("请先在设置中配置 API Key".to_string());
    }

    let client = reqwest::Client::new();

    let system_prompt = format!(
        r#"你是一个 Python Pandas 数据分析专家。
当前处理的 Excel 表格包含列: [{}]。
Dataframe 变量名为 `df`。

任务：将用户的自然语言转换为 Python 代码。
要求：
1. 只返回代码，不要 markdown，不要解释。
2. 必须修改 `df` 或将结果赋值给 `result`。
3. 如果是绘图需求，请忽略，只做数据处理。
4. 只能使用 pandas (pd) 和 numpy (np)。

示例：
用户: "算一下 Age 的平均值"
你: result = df['Age'].mean()

用户: "删除 Name 列"
你: df.drop(columns=['Name'], inplace=True); result = "已删除 Name 列"
"#,
        columns
    );

    let req_body = ChatRequest {
        model: model_name,
        messages: vec![
            MessageApi {
                role: "system".into(),
                content: system_prompt,
            },
            MessageApi {
                role: "user".into(),
                content: prompt,
            },
        ],
    };

    let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));

    let res = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&req_body)
        .send()
        .await
        .map_err(|e| format!("网络错误: {}", e))?;

    // ✅ 修复点：先保存状态码，再消耗 response 读取文本
    let status = res.status();
    if !status.is_success() {
        let error_text = res.text().await.unwrap_or_default();
        return Err(format!("API 错误 (Status {}): {}", status, error_text));
    }

    let json: ChatResponse = res.json().await.map_err(|e| format!("解析失败: {}", e))?;

    if let Some(choice) = json.choices.first() {
        let code = choice
            .message
            .content
            .clone()
            .replace("```python", "")
            .replace("```", "")
            .trim()
            .to_string();
        Ok(code)
    } else {
        Err("AI 未返回内容".into())
    }
}

// --- 4. UI 主组件 ---
#[component]
fn App() -> Element {
    let mut current_view = use_signal(|| View::Chat);
    let mut input_text = use_signal(|| String::new());
    let mut is_dragging = use_signal(|| false);
    let mut is_loading = use_signal(|| false);

    let mut last_file_path = use_signal(|| String::new());

    let mut api_key = use_signal(|| String::new());
    let mut base_url = use_signal(|| "https://api.moonshot.cn/v1".to_string());
    let mut model_name = use_signal(|| "moonshot-v1-8k".to_string());

    let mut messages = use_signal(|| {
        vec![
        ChatMessage {
            id: 0,
            text: "👋 嗨！我是你的智能 Excel 助手。\n\n⚠️ 请务必在设置中检查 'Base URL' 和 'Model Name' 是否正确！".into(),
            is_user: false
        }
    ]
    });

    rsx! {
        document::Stylesheet { href: asset!("/assets/main.css") }

        div {
            class: "app-container",
            ondragover: move |evt| { evt.prevent_default(); if !is_dragging() { is_dragging.set(true); } },
            ondragleave: move |evt| { evt.prevent_default(); is_dragging.set(false); },
            ondrop: move |evt| {
                evt.prevent_default();
                is_dragging.set(false);
                let files = evt.data().files();
                if let Some(first_file) = files.first() {
                    let file_name = first_file.name();
                    let current_dir = std::env::current_dir().unwrap();
                    let full_path = current_dir.join(&file_name).to_str().unwrap().to_string();

                    last_file_path.set(full_path.clone());

                    let new_id = messages.read().len();
                    messages.write().push(ChatMessage {
                        id: new_id,
                        text: format!("📂 已加载文件: {}\n你可以问我：'算出平均销售额' 或 '删掉空行'", file_name),
                        is_user: false
                    });
                }
            },

            div { class: "sidebar",
                div { class: if current_view() == View::Chat { "nav-icon active" } else { "nav-icon" }, onclick: move |_| current_view.set(View::Chat), "💬" }
                div { class: if current_view() == View::Settings { "nav-icon active" } else { "nav-icon" }, onclick: move |_| current_view.set(View::Settings), "⚙️" }
            }

            div { class: "content-area",
                if is_dragging() { div { class: "drag-overlay", "📂 投喂 Excel 给 AI！" } }

                if is_loading() {
                    div {
                        style: "position: absolute; top: 10px; right: 10px; background: #ff69b4; color: white; padding: 5px 10px; border-radius: 12px; font-size: 12px; z-index: 999;",
                        "🧠 AI 正在思考..."
                    }
                }

                if current_view() == View::Chat {
                    div { class: "chat-scroll",
                        for msg in messages.read().iter() {
                            div {
                                key: "{msg.id}",
                                class: if msg.is_user { "message msg-user" } else { "message msg-ai" },
                                style: "white-space: pre-wrap;",
                                "{msg.text}"
                            }
                        }
                    }

                    div { class: "input-section",
                        input {
                            placeholder: "输入需求...",
                            value: "{input_text}",
                            oninput: move |evt| input_text.set(evt.value()),
                            disabled: is_loading(),
                            onkeydown: move |evt| {
                                if evt.key() == Key::Enter && !input_text.read().is_empty() {
                                    let user_prompt = input_text.read().clone();
                                    let file_path = last_file_path.read().clone();
                                    let key = api_key.read().clone();
                                    let url = base_url.read().clone();
                                    let model = model_name.read().clone();

                                    let new_id = messages.read().len();
                                    messages.write().push(ChatMessage { id: new_id, text: user_prompt.clone(), is_user: true });
                                    input_text.set(String::new());

                                    if file_path.is_empty() {
                                        let err_id = messages.read().len();
                                        messages.write().push(ChatMessage { id: err_id, text: "请先拖入一个 Excel 文件！".into(), is_user: false });
                                        return;
                                    }

                                    is_loading.set(true);

                                    spawn(async move {
                                        let columns = get_excel_columns(&file_path);
                                        let ai_result = call_ai(key, url, model, user_prompt, columns).await;

                                        match ai_result {
                                            Ok(code) => {
                                                let exec_result = run_python_code(&file_path, &code);
                                                let final_reply = format!("🔧 执行代码:\n{}\n\n📊 结果:\n{}", code, exec_result);
                                                let ai_id = messages.read().len();
                                                messages.write().push(ChatMessage { id: ai_id, text: final_reply, is_user: false });
                                            }
                                            Err(err) => {
                                                let err_id = messages.read().len();
                                                messages.write().push(ChatMessage { id: err_id, text: format!("❌ 失败: {}", err), is_user: false });
                                            }
                                        }
                                        is_loading.set(false);
                                    });
                                }
                            }
                        },
                        button {
                            class: "btn-send",
                            onclick: move |_| {
                                if !input_text.read().is_empty() {
                                    let user_prompt = input_text.read().clone();
                                    let file_path = last_file_path.read().clone();
                                    let key = api_key.read().clone();
                                    let url = base_url.read().clone();
                                    let model = model_name.read().clone();

                                    let new_id = messages.read().len();
                                    messages.write().push(ChatMessage { id: new_id, text: user_prompt.clone(), is_user: true });
                                    input_text.set(String::new());

                                    if file_path.is_empty() {
                                        let err_id = messages.read().len();
                                        messages.write().push(ChatMessage { id: err_id, text: "请先拖入一个 Excel 文件！".into(), is_user: false });
                                        return;
                                    }

                                    is_loading.set(true);

                                    spawn(async move {
                                        let columns = get_excel_columns(&file_path);
                                        let ai_result = call_ai(key, url, model, user_prompt, columns).await;

                                        match ai_result {
                                            Ok(code) => {
                                                let exec_result = run_python_code(&file_path, &code);
                                                let final_reply = format!("🔧 执行代码:\n{}\n\n📊 结果:\n{}", code, exec_result);
                                                let ai_id = messages.read().len();
                                                messages.write().push(ChatMessage { id: ai_id, text: final_reply, is_user: false });
                                            }
                                            Err(err) => {
                                                let err_id = messages.read().len();
                                                messages.write().push(ChatMessage { id: err_id, text: format!("❌ 失败: {}", err), is_user: false });
                                            }
                                        }
                                        is_loading.set(false);
                                    });
                                }
                            },
                            "发送"
                        }
                    }
                } else if current_view() == View::Settings {
                    div { class: "settings-panel",
                        h2 { "🤖 Moonshot (Kimi) 配置" }

                        div { class: "settings-group",
                            label { "API Base URL (必须以 /v1 结尾)" }
                            input { value: "{base_url}", oninput: move |evt| base_url.set(evt.value()) }
                            p { style: "color: #666; font-size: 11px;", "正确示例: https://api.moonshot.cn/v1" }
                        }

                        div { class: "settings-group",
                            label { "Model Name (模型名称)" }
                            input { value: "{model_name}", oninput: move |evt| model_name.set(evt.value()) }
                            p { style: "color: #666; font-size: 11px;", "常用: moonshot-v1-8k, moonshot-v1-32k, kimi-k2-thinking" }
                        }

                        div { class: "settings-group",
                            label { "API Key" }
                            input { type: "password", value: "{api_key}", oninput: move |evt| api_key.set(evt.value()), placeholder: "sk-..." }
                        }
                    }
                }
            }
        }
    }
}
