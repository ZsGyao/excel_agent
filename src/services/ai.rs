use crate::{models::AppConfig, services::excel_engine::FileSchema};
use anyhow::Result;
use reqwest::{self, Client};
use serde_json::{self, json, Value};
use std::collections::HashMap;
use tracing::debug; // 确保 main.rs 中有 mod services;

const PROMPT_TEMPLATE: &str = include_str!("../../assets/prompt_coder.md");

const PANDAS_TEMPLATE: &str = r#"
【后台极速静默模式 (Pandas)】
代码骨架参考：
import pandas as pd

def get_real_col(df, semantic_name):
        """精准拆分函数：依靠特殊契约符号 ||| 还原最底层的真实物理列名"""
        # 如果大模型自作聪明传了短名字，刚好匹配上，直接返回
        if semantic_name in df.columns:
            return semantic_name
            
        # 核心逻辑：用我们约定的特殊符号拆分，取数组的最后一部分！
        # 例如: "总表|||二级表\r\n说明|||消保信访".split("|||")[-1] => "消保信访"
        # 这样 100% 保留了原有的换行符、空格等任何特殊字符
        physical_col = semantic_name.split("@|||@")[-1]
        
        # 遍历真实的 DataFrame 列名进行绝对相等的比对
        for col in df.columns:
            if str(col) == physical_col:
                return col
                
        # 兜底返回
        return physical_col

try:
    # 路径直接从 JSON Key 复制，不要瞎编
    file_path = r"绝对路径"
    print("⏳ 正在后台极速处理...")
    
    # 获取表头所在的物理行 (JSON 中的 data_start_row - 1)
    df = pd.read_excel(file_path, sheet_name="Sheet名", header=2) # 假设 data_start_row 是 3，则 header 是 2
    
    # 示例：获取真实列名
    # real_col = get_real_col(df, "JSON里那个巨长的列名")
    # df[real_col] = ...
    
    # ... 你的业务逻辑 ...
    
    print("💾 正在保存结果...")
    with pd.ExcelWriter(file_path, engine='openpyxl', mode='a', if_sheet_exists='replace') as writer:
        df.to_excel(writer, sheet_name="处理后的Sheet名", index=False)
        
    print("✨ 任务执行成功！")
except Exception as e:
    print(f"❌ 执行失败: {e}")
"#;

const XLWINGS_TEMPLATE: &str = r#"
【前台热更新模式 (xlwings)】
⚠️ 必须使用 xlwings，但数据处理必须依托 pandas！严禁直接使用 sheet.range() 循环修改单个单元格！

代码骨架参考：
import pythoncom
# 初始化当前线程的 Windows COM 组件
pythoncom.CoInitialize()

try:
    import xlwings as xw
    import pandas as pd
    import os

    def get_real_col(df, semantic_name):
        """精准拆分函数：完全信任约定契约，绝不破坏用户原始数据的任何空格或符号"""
        if semantic_name in df.columns:
            return semantic_name
            
        # 1. 绝对精准还原
        physical_col = semantic_name.split("@|||@")[-1]
        if physical_col in df.columns:
            return physical_col
            
        # 2. 极端兜底 (防止 Pandas 给同名列加了 .1 后缀，或 Excel 物理表头真的带有不可见的回车)
        for col in df.columns:
            if str(col).strip() == physical_col.strip():
                return col
                
        return physical_col

    file_path = r"绝对路径"
    file_name = os.path.basename(file_path)
    print("⏳ 正在通过 xlwings 连接当前打开的 Excel...")
    
    wb = xw.books[file_name] 
    sheet = wb.sheets["Sheet名"]
    
    start_row = 3 # 替换为真实的 data_start_row
    
    # 🌟 核心修复：放弃脆弱的 expand='table'，使用 used_range 绝对物理坐标锁定整个表格区域！
    max_row = sheet.used_range.last_cell.row
    max_col = sheet.used_range.last_cell.column
    df = sheet.range((start_row, 1), (max_row, max_col)).options(pd.DataFrame, header=1, index=False).value
    
    # 2. 匹配真实列名并处理业务逻辑
    # real_col_1 = get_real_col(df, "长列名1")
    # ...
    
    # 3. 整体写回
    sheet.range((start_row, 1)).options(index=False).value = df
    
    print("✨ Excel 界面热更新完成！请在 Excel 窗口中查看 (无需保存)。")
except Exception as e:
    print(f"❌ xlwings 热更新失败: {e}")
finally:
    # 释放当前线程的 COM 资源
    pythoncom.CoUninitialize()
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
            "temperature": 0.1, // 极低温度，保证代码生成的确定性
            "max_tokens": 4096
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
        .replace("{{USER_QUERY}}", user_query);

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
