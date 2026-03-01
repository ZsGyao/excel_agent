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
⚠️ 必须使用 xlwings！必须调用提供的 load_safe_df 函数安全加载数据！

import pythoncom
# 初始化当前线程的 Windows COM 组件（防止 Rust Tokio 异步多线程调用 Excel 时崩溃）
pythoncom.CoInitialize()

try:
    import xlwings as xw
    import pandas as pd
    import os
    import json

    # ==========================================
    # 核心机制 1：后期绑定数据字典
    # ==========================================
    # ⚠️ 绝对禁止 AI 修改这行代码的占位符！
    # 解释：由 Rust 底层在执行代码的前一毫秒，将真实的上万字 JSON 瞬间替换到这个词里。
    # 目的：防止 AI 抄写巨长 JSON 时发生截断（SyntaxError）。
    schema_json = json.loads(r'''{{SCHEMA_JSON}}''')
    
    # ==========================================
    # 核心机制 2：安全读取引擎（绕过合并单元格）
    # ==========================================
    def load_safe_df(wb, file_path, sheet_name):
        """安全读取指定 Sheet 的纯数据，并强制赋予全语义字典表头"""
        # 1. 从注入的字典中提取该表的元数据
        sheet_schema = schema_json[file_path]["sheets"][sheet_name]
        start_row = sheet_schema["data_start_row"] + 1
        semantic_columns = sheet_schema["columns"]
        
        sheet = wb.sheets[sheet_name]
        
        # 2. 【防 OOM 炸弹】：用 Rust 提取的真实行数做上限，防止 Excel 幽灵空行撑爆内存
        safe_max_row = start_row + sheet_schema["total_rows"] - 1
        max_row = min(sheet.used_range.last_cell.row, safe_max_row)
        max_col = len(semantic_columns)
        
        # 如果是空表，直接返回带好表头的空 DataFrame
        if max_row < start_row: 
            return pd.DataFrame(columns=semantic_columns)
            
        # 3. 【无视合并单元格】：绝对物理坐标框选，跳过所有表头（header=False），只读纯数据！
        df = sheet.range((start_row, 1), (max_row, max_col)).options(pd.DataFrame, header=False, index=False).value
        
        # 4. 【维度防撕裂】：防止用户在表末尾乱敲空格，强行按字典长度截断数据列
        df = df.iloc[:, :len(semantic_columns)]
        
        # 5. 【强行戴帽子】：把字典里绝对正确的超长语义列名，盖在纯数据头上
        df.columns = semantic_columns
        return df

    # ==========================================
    # 核心机制 3：AI 幻觉拯救器（短名寻路导弹）
    # ==========================================
    def get_col_name(df, keyword):
        """让 AI 只需传入最短的物理列名（如'部门'），自动匹配出超长语义列名"""
        # 第一优先级：精确匹配契约符号 @|||@ 拆分后的最后一部分
        for col in df.columns:
            if str(col).split("@|||@")[-1].strip() == keyword.strip(): 
                return col
        # 第二优先级：模糊包含匹配
        for col in df.columns:
            if keyword.strip() in str(col): 
                return col
        # 兜底：原样返回，让 Pandas 报错提醒
        return keyword

    print("⏳ 正在通过 xlwings 连接 Excel 引擎...")
    
    # ==========================================
    # 🤖 AI 业务代码区 (以下由 AI 动态生成)
    # ==========================================
    # 示例：获取当前处理的文件路径并连接 Workbook
    # file_path = list(schema_json.keys())[0]  
    # wb = xw.Book(file_path)
    # df = load_safe_df(wb, file_path, "Sheet1")
    # 
    # 【需求示例：清空“消保信访”列中不等于“2026学习”的数据】
    # col_full_name = get_col_name(df, "消保信访")
    # df.loc[df[col_full_name] != '2026学习', col_full_name] = ""
    # 
    # 【安全写回示例（极其重要）】
    # 算出该列在 Excel 中的绝对物理列号 (从 1 开始算，所以要 +1)
    # col_idx = list(df.columns).index(col_full_name) + 1
    # 仅把修改后的这一列怼回原位，绝不覆盖其他地方的公式和格式！
    # sheet = wb.sheets["Sheet1"]
    # sheet.range((start_row, col_idx)).options(index=False, header=False).value = df[col_full_name].values.reshape(-1, 1)
    # ==========================================

    print("✨ Excel 界面热更新完成！")
except Exception as e:
    print(f"❌ xlwings 热更新失败: {e}")
finally:
    # 无论成功失败，必须释放 COM 锁，否则 Excel 进程会残留
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
            let executable_code = clean_code.replace("{{SCHEMA_JSON}}", &schema_json);
            Ok(executable_code)
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
