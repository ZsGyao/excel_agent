use crate::{
    models::{AppConfig, IntentType, OrchestratorResponse},
    services::excel_engine::{FileSchema, SHEET_JOIN_STR},
};
use anyhow::Result;
use regex::Regex;
use reqwest::{self, Client};
use serde_json::{self, json, Value};
use std::{collections::HashMap, fs, path::PathBuf, sync::OnceLock};
use tracing::{debug, info}; // 确保 main.rs 中有 mod services;

const ORCHESTRATOR_PROMPT: &str = include_str!("../../assets/prompts/orchestrator.md");
const CODER_PROMPT: &str = include_str!("../../assets/prompts/coder.md");
const UNIVERSAL_SANDBOX_TEMPLATE: &str =
    include_str!("../../assets/templates/universal_sandbox.py");

/// 全局静态正则引擎，使用 OnceLock 确保只编译一次，提升高并发下的执行性能。
static REF_REGEX: OnceLock<Regex> = OnceLock::new();

/// 🚀 后端指令解包拦截器 (增强版：支持层级分类识别)
///
/// # 参数
/// * `raw_query` - 原始查询字符串
/// * `schemas` - 全局文件 Schema 映射，用于校验引用是“具体列”还是“分类节点”
pub fn preprocess_query_with_refs(
    raw_query: &str,
    schemas: &HashMap<String, FileSchema>,
) -> String {
    let re = REF_REGEX.get_or_init(|| {
        Regex::new(r"\[\[REF:(.*?)\|(.*?)\|(.*?)\]\]").expect("REF_REGEX 编译失败")
    });

    if !re.is_match(raw_query) {
        return raw_query.to_string();
    }

    let mut system_hints = String::from(
        "【⚠️ 确定性操作指令 - 优先级最高 ⚠️】\n\
        用户已在 GUI 界面明确锁定了以下物理操作目标。你必须直接使用这些绝对路径：\n",
    );

    let processed_query = re.replace_all(raw_query, |caps: &regex::Captures| {
        let file_path = &caps[1];
        let sheet_name = &caps[2];
        let col_path = &caps[3];

        if !col_path.is_empty() {
            // 🌟 核心逻辑：判断当前引用是“叶子列”还是“分类前缀”
            let mut is_category = false;
            if let Some(file_schema) = schemas.get(file_path) {
                if let Some(sheet_schema) = file_schema.sheets.get(sheet_name) {
                    // 判断准则：只要有任何物理列是以“当前路径 + 分隔符”开头的，它就是一个分类
                    let category_prefix = format!("{}{}", col_path, SHEET_JOIN_STR);
                    is_category = sheet_schema
                        .columns
                        .iter()
                        .any(|c| c.semantic_name.starts_with(&category_prefix));
                }
            }

            if is_category {
                // 1. 批量锁定 (分类节点)
                system_hints.push_str(&format!(
                    "- 🎯 批量锁定分类: 文件 `{}` -> 表 `{}` -> 分类前缀 `{}`\n\
                     - 执行要求: 你必须找到所有以此前缀开头的列，并对它们执行相同操作。\n",
                    file_path, sheet_name, col_path
                ));
                format!("“{}”分类下的所有数据", col_path)
            } else {
                // 2. 精确锁定 (叶子节点)
                system_hints.push_str(&format!(
                    "- 🎯 锁定物理列: 文件 `{}` -> 表 `{}` -> 绝对列名 `{}`\n",
                    file_path, sheet_name, col_path
                ));
                let short_col = col_path.split(SHEET_JOIN_STR).last().unwrap_or(col_path);
                format!("`{}`表的`{}`列", sheet_name, short_col)
            }
        } else if !sheet_name.is_empty() {
            // 表级锁定
            system_hints.push_str(&format!(
                "- 🎯 锁定物理表: 文件 `{}` -> 表 `{}`\n",
                file_path, sheet_name
            ));
            format!("`{}`工作表", sheet_name)
        } else {
            // 文件级锁定
            system_hints.push_str(&format!("- 🎯 锁定整个文件: `{}`\n", file_path));
            format!("文件 `{}`", file_path)
        }
    });

    format!(
        "{}\n\n### 用户的最终需求如下：\n{}",
        system_hints, processed_query
    )
}

async fn llm_request(config: &AppConfig, system_prompt: &str, user_prompt: &str) -> Result<String> {
    let profile = config.active_profile();
    let client = Client::new();
    let response = client
        .post(format!(
            "{}/chat/completions",
            profile.base_url.trim_end_matches('/')
        ))
        .header("Authorization", format!("Bearer {}", profile.api_key))
        .json(&json!({
            "model": profile.model_id,
            "messages": [
                { "role": "system", "content": system_prompt },
                { "role": "user", "content": user_prompt }
            ],
            "temperature": 0.1,
            "max_token": 4096
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

/// 清理大模型回复中可能附带的 Markdown 代码块标记
fn clean_markdown_code(raw_code: &str) -> String {
    let mut code = raw_code.trim().to_string();
    if code.starts_with("```json") {
        code = code.trim_start_matches("```json").trim_start().to_string();
    }
    if code.starts_with("```python") {
        code = code
            .trim_start_matches("```python")
            .trim_start()
            .to_string();
    }
    if code.starts_with("```") {
        code = code.trim_start_matches("```").trim_start().to_string();
    }
    if code.ends_with("```") {
        code = code.trim_end_matches("```").trim_end().to_string();
    }
    code
}

/// 复合指令执行引擎的主入口 (Orchestrator Pipeline)
pub async fn call_ai(
    config: &AppConfig,
    user_query: &str,
    schema_json: &str,
    _is_file_opened: bool,
    schemas: &HashMap<String, FileSchema>,
) -> Result<String, String> {
    info!("🧭 [阶段 1: 挂载沙盒] 将当前 Excel 架构写入离线缓存...");
    let context_path = PathBuf::from(".agent_context.json");
    if let Err(e) = fs::write(&context_path, schema_json) {
        return Err(format!("文件系统错误: 无法写入沙盒上下文 ({})", e));
    }

    let refined_query = preprocess_query_with_refs(user_query, schemas);

    debug!("Refined_query: {}\n", &refined_query);

    info!("🧭 [阶段 2: 任务编排] 呼叫 Orchestrator 拆解复杂意图...");
    let orchestrator_res = llm_request(config, ORCHESTRATOR_PROMPT, refined_query.as_str())
        .await
        .unwrap_or_else(|_| {
            r#"{"tasks": [{"step":1, "intent":"TYPE_UPDATE", "description":"处理默认需求"}]}"#
                .to_string()
        });

    // 将返回的 JSON 强转为任务流水线结构体
    let plan: OrchestratorResponse = serde_json::from_str(&clean_markdown_code(&orchestrator_res))
        .unwrap_or_else(|e| {
            debug!("Orchestrator JSON 解析失败: {}", e);
            OrchestratorResponse {
                tasks: vec![crate::models::Task {
                    step: 1,
                    intent: IntentType::Update,
                    description: refined_query.clone(),
                }],
            }
        });

    info!(
        "🎯 [阶段 3: 提取约束] 共拆解出 {} 个流水线步骤",
        plan.tasks.len()
    );
    info!("PLAN -> {:?}\r\n", plan);

    let mut step_guides = String::new();
    let mut unified_constraints = String::new();

    for task in &plan.tasks {
        step_guides.push_str(&format!("【步骤 {}】: {}\n", task.step, task.description));
        let constraint = task.intent.get_constraint();
        if !unified_constraints.contains(constraint) {
            unified_constraints.push_str(&format!("{}\n", constraint));
        }
    }

    info!("🧠 [阶段 4: 代码生成] 组装沙盒规则，呼叫 Coder...");
    // 模板渲染：将任务和约束塞入写代码模型的 Prompt
    let coder_system_prompt = CODER_PROMPT
        .replace("{{TASK_STEPS}}", &step_guides)
        .replace("{{SANDBOX_CONSTRAINTS}}", &unified_constraints)
        .replace("{{SCHEMA_JSON}}", schema_json);

    match llm_request(config, &coder_system_prompt, refined_query.as_str()).await {
        Ok(raw_code) => {
            let ai_business_code = clean_markdown_code(&raw_code);
            // 模板渲染：将 AI 生成的业务代码注入安全的 Python 沙盒底座
            let executable_code = UNIVERSAL_SANDBOX_TEMPLATE
                .replace("{{AI_BUSINESS_CODE}}", &ai_business_code)
                .replace("\r\n", "\n");
            info!("✅ [阶段 5: 组装成功] 沙盒代码打包完毕，准备运行！");
            Ok(executable_code)
        }
        Err(e) => Err(format!("Coder 模型请求失败: {}", e)),
    }
}

/// 核心组装逻辑：将底层的复杂 FileSchema 转化为 AI 专用的精简版 JSON
pub fn generate_dehydrated_schema_json(active_schemas: &HashMap<String, FileSchema>) -> String {
    let mut context_map = serde_json::Map::new();

    for (file_path, file_schema) in active_schemas {
        let mut sheets_map = serde_json::Map::new();

        for (sheet_name, sheet_schema) in &file_schema.sheets {
            // 2. 剥离物理结构：把 ColumnMapping 数组，变成纯纯的 String 数组
            let clean_columns: Vec<String> = sheet_schema
                .columns
                .iter()
                .map(|c| c.semantic_name.clone())
                .collect();

            // 3. 使用 json! 宏，像写 JS 一样自由拼装我们想要的层级
            let sheet_json = json!({
                "data_start_row": sheet_schema.data_start_row,
                "total_rows": sheet_schema.total_rows,
                "columns": clean_columns, // 只有名字，没有坐标！
                "preview_data": sheet_schema.preview_data // 前3行的预览数据
            });

            sheets_map.insert(sheet_name.clone(), sheet_json);
        }

        // 4. 将所有的 sheet 塞入这个文件节点下
        let file_json = json!({
            "sheets": sheets_map
        });

        // 5. 将文件节点塞入全局上下文中
        context_map.insert(file_path.clone(), file_json);
    }

    // 将组装好的 JSON 对象转换为漂亮的带缩进的字符串
    let final_json = Value::Object(context_map);
    serde_json::to_string_pretty(&final_json).unwrap_or_default()
}
