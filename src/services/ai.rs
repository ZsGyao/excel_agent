use crate::models::AppConfig;
use crate::services::python;
use anyhow::Result;
use reqwest::{self, Client};
use serde_json::{self, json, Value};
use std::{fs::read_to_string, path::Path}; // 确保 main.rs 中有 mod services;

/// 内部 helper: 读取 Prompt 模板
fn load_prompt_template(filename: &str) -> String {
    let path = Path::new("assets").join(filename);
    read_to_string(path).unwrap_or_else(|_| {
        println!("⚠️ Warning: Prompt file {} not found!", filename);
        // 如果找不到，返回空字符串，依靠 LLM 的泛化能力
        String::new()
    })
}

/// 内部 helper: 基础 LLM 调用
async fn llm_request(config: &AppConfig, system_prompt: &str, user_prompt: &str) -> Result<String> {
    let profile = config.active_profile();
    let api_key = &profile.api_key;
    let base_url = &profile.base_url;
    let model = &profile.model_id;

    let client = Client::new();

    // 构造请求，保持低温以确保稳定
    let response = client
        .post(format!(
            "{}/chat/completions",
            base_url.trim_end_matches('/')
        ))
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&json!({
            "model": model,
            "messages": [
                { "role": "system", "content": system_prompt },
                { "role": "user", "content": user_prompt }
            ],
            "temperature": 0.1
        }))
        .send()
        .await?;

    if !response.status().is_success() {
        return Ok(format!("API Error: {}", response.text().await?));
    }

    let json: Value = response.json().await?;
    Ok(json["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("")
        .to_string())
}

/// 主入口: 智能 Re-Act 循环 (生成代码版)
///
/// 逻辑：
/// 1. 侦察 (Peek) -> 2. 规划 (Plan) -> 3. 编码 (Code) -> 4. 返回前端 (Return)
/// **注意：不自动执行代码，交由用户确认。**
pub async fn call_ai(
    config: &AppConfig,
    user_content: &str,
    context_file_path: Option<String>,
) -> Result<String> {
    // 1. 如果没有文件上下文，直接进行普通闲聊
    let file_path = match context_file_path {
        Some(path) => path,
        None => {
            // 使用默认 System Prompt
            let sys_prompt = load_prompt_template("system_prompt.md");
            return llm_request(config, &sys_prompt, user_content).await;
        }
    };

    println!("🚀 启动 Re-Act 生成流程: {}", file_path);

    // --- STEP 1: 感知 (Peek) ---
    // 调用 Python 获取前 20 行数据指纹，用于辅助决策
    println!("👀 [Step 1] 正在侦察 Excel 结构...");
    let peek_json_str = python::peek_excel(&file_path)
        .await
        .unwrap_or_else(|e| format!("{{'status': 'error', 'msg': '{}'}}", e));

    // --- STEP 2: 思考 (Plan) ---
    // 让 LLM 分析表头结构，决定 header_count
    println!("🧠 [Step 2] 正在规划任务...");
    let planner_tmpl = load_prompt_template("prompt_planner.md");
    let user_msg_plan = format!(
        "User Query: {}\nCSV Preview:\n{}",
        user_content, peek_json_str
    );
    // 如果没有 planner 模板，跳过这一步（降级处理）
    let plan_json = if !planner_tmpl.is_empty() {
        llm_request(config, &planner_tmpl, &user_msg_plan).await?
    } else {
        println!("⚠️ 未找到 prompt_planner.md，跳过规划步骤");
        "{}".to_string()
    };
    println!("💡 规划结果: {}", plan_json);

    // --- STEP 3: 编码 (Code) ---
    // 根据规划结果生成最终 Python 代码
    println!("💻 [Step 3] 正在生成代码...");
    let coder_tmpl = load_prompt_template("prompt_coder.md");

    // 如果没有 coder 模板，回退到默认 prompt
    if coder_tmpl.is_empty() {
        let sys_prompt = load_prompt_template("system_prompt.md");
        let fallback_ctx = format!("Target File: {}\nStructure Hint: {}", file_path, plan_json);
        return llm_request(
            config,
            &sys_prompt,
            &format!("{}\n\nContext:\n{}", user_content, fallback_ctx),
        )
        .await;
    }

    let user_msg_code = format!(
        "Structure Config: {}\nUser Query: {}",
        plan_json, user_content
    );

    // 注入文件路径
    let coder_tmpl_filled = coder_tmpl.replace("{file_path}", &file_path.replace("\\", "\\\\"));

    let code_response = llm_request(config, &coder_tmpl_filled, &user_msg_code).await?;

    // --- STEP 4: 返回 (Return) ---
    // 直接返回生成的 Markdown 代码块。
    // 前端 UI 会识别 ```python，并显示“运行”按钮。
    println!("✅ 代码生成完毕，等待用户确认");

    // 可选：在返回内容前加一点分析摘要，让用户知道 AI 是怎么想的
    // let final_response = format!("**分析完毕**：检测到表格结构配置为 `{}`。\n\n{}", plan_json, code_response);

    // 为了保持界面简洁，直接返回代码部分即可，或者只包含必要的解释
    Ok(code_response)
}
