use crate::services::python;
use crate::{models::AppConfig, services::excel_engine::FileSchema};
use anyhow::Result;
use reqwest::{self, Client};
use serde_json::{self, json, Value};
use std::{collections::HashMap, fs::read_to_string, path::Path};
use tracing::debug; // 确保 main.rs 中有 mod services;

const PROMPT_TEMPLATE: &str = include_str!("../../assets/prompt_coder.md");

const PANDAS_TEMPLATE: &str = r#"
【后台极速静默模式 (Pandas / Openpyxl)】
文件当前未被占用。
- 【原表数据清洗/修改】：使用 `pandas` 读取，修改后使用 `to_excel` 覆盖原文件。
- 【新建统计/透视表】：使用 `pandas` 计算，必须使用 `pd.ExcelWriter(mode='a', engine='openpyxl')` 写入该文件的新 Sheet 中，严禁覆盖原始数据 Sheet。
- 【单元格样式修改】：强制使用 `openpyxl`。

代码骨架参考：
import pandas as pd
try:
    file_path = r"从JSON中获取的绝对路径"
    print("⏳ 正在后台极速处理...")
    df = pd.read_excel(file_path, sheet_name="从JSON中获取的Sheet名")
    
    # ... 你的逻辑 ...
    
    # 保存结果
    with pd.ExcelWriter(file_path, engine='openpyxl', mode='a', if_sheet_exists='replace') as writer:
        df.to_excel(writer, sheet_name="处理后的Sheet名", index=False)
        
    print("✨ 任务执行成功！")
except Exception as e:
    print(f"❌ 执行失败: {e}")
"#;

const XLWINGS_TEMPLATE: &str = r#"
【前台热更新模式 (xlwings)】
⚠️ 极其重要：系统检测到用户当前正在打开并浏览该 Excel 文件！
你绝对不能使用 pandas 的 `to_excel` 或 openpyxl 的 `wb.save()`，这会触发 PermissionError。
你必须使用 `xlwings` 连接到当前活动的 Excel 窗口进行热更新！

代码骨架参考：
import xlwings as xw
import pandas as pd
import os

try:
    file_path = r"从JSON中获取的绝对路径"
    file_name = os.path.basename(file_path)
    print("⏳ 正在通过 xlwings 连接当前打开的 Excel...")
    
    wb = xw.books[file_name] 
    sheet = wb.sheets["从JSON中获取的Sheet名"]
    
    # ... 你的业务逻辑 (可以直接操作 sheet.range，或读入 pd.DataFrame 处理后写回) ...
    # df = sheet.range('A1').options(pd.DataFrame, header=1, index=False, expand='table').value
    # sheet.range('A1').options(index=False).value = df
    
    print("✨ Excel 界面热更新完成！请在 Excel 窗口中查看 (无需保存)。")
except Exception as e:
    print(f"❌ xlwings 热更新失败: {e}")
"#;

async fn llm_request(config: &AppConfig, system_prompt: &str, user_prompt: &str) -> Result<String> {
    let profile = config.active_profile();
    let api_key = &profile.api_key;
    let base_url = &profile.base_url;
    let model = &profile.model_id;

    let client = Client::new();

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
            "temperature": 0.1 // 极低温度，保证代码生成的确定性
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

/// 内部 helper: 清理 LLM 可能生成的 Markdown 标记
fn clean_markdown_code(raw_code: &str) -> String {
    let mut code = raw_code.trim().to_string();
    if code.starts_with("```python") {
        code = code
            .trim_start_matches("```python")
            .trim_start()
            .to_string();
    } else if code.starts_with("```") {
        code = code.trim_start_matches("```").trim_start().to_string();
    }
    if code.ends_with("```") {
        code = code.trim_end_matches("```").trim_end().to_string();
    }
    code
}

pub async fn call_ai(
    config: &AppConfig,
    user_query: &str,
    schema_json: &str,
    is_file_opened: bool, // 🌟 接收来自 Rust 探针的情报
) -> Result<String, String> {
    // 1. 根据探针结果，选择注入哪个底层模板
    let target_template = if is_file_opened {
        XLWINGS_TEMPLATE
    } else {
        PANDAS_TEMPLATE
    };

    // 2. 组装 System Prompt
    // 将占位符替换掉。因为我们把 user_query 放在 user 角色里发，
    // 所以把模板里的 {{USER_QUERY}} 替换为空，保持 System Prompt 纯净。
    let system_prompt = PROMPT_TEMPLATE
        .replace("{{SCHEMA_JSON}}", schema_json)
        .replace("{{EXECUTION_TEMPLATE}}", target_template)
        .replace("{{USER_QUERY}}", "");

    println!("🧠 [AI 思考中] 正在根据最新脱水 JSON 生成代码...");
    if is_file_opened {
        println!("🔥 [AI 模式] 挂载热更新 xlwings 引擎");
    } else {
        println!("⚡ [AI 模式] 挂载后台静默 pandas 引擎");
    }

    debug!("----- SYSTEM PROMPT: --------\n {:?}\n", system_prompt);

    // 3. 发送请求
    match llm_request(config, &system_prompt, user_query).await {
        Ok(raw_response) => {
            debug!("----- LLM REQUEST: --------\n {:?}\n", raw_response);

            // 4. 清洗代码（防止大模型强行输出 ```python 标记导致 exec 报错）
            let clean_code = clean_markdown_code(&raw_response);
            println!("✅ [AI 响应] 代码生成完毕！");
            Ok(clean_code)
        }
        Err(e) => Err(format!("网络请求失败: {}", e)),
    }
}

/// 核心组装逻辑：将底层的复杂 FileSchema 转化为 AI 专用的精简版 JSON
pub fn generate_dehydrated_schema_json(active_schemas: &HashMap<String, FileSchema>) -> String {
    let mut context_map = serde_json::Map::new();

    for (file_path, file_schema) in active_schemas {
        // 1. 提取文件名 (把冗长的 D:\XXX\报名表.xlsx 变成 报名表.xlsx，给 AI 减负)
        let file_name = std::path::Path::new(file_path)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

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
        context_map.insert(file_name, file_json);
    }

    // 将组装好的 JSON 对象转换为漂亮的带缩进的字符串
    let final_json = Value::Object(context_map);
    serde_json::to_string_pretty(&final_json).unwrap_or_default()
}
