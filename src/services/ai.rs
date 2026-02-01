use anyhow::Result;
use reqwest::{self, Client};
use serde_json::{self, json, Value};
use std::{
    fs::{self, read_to_string},
    path::Path,
};

use crate::models::AppConfig;

// 内置兜底 Prompt，防止文件被误删后软件直接崩溃
const DEFAULT_FALLBACK_PROMPT: &str = r#"
# 角色设定
你是一个拥有 10 年经验的 Python 数据分析专家，也是一个 Excel 自动化大师。
你的任务是根据用户的需求，判断是进行普通对话，还是编写 Python 代码来处理 Excel 数据。

# 核心交互规则 (请严格遵守)
1. **普通闲聊/解释**：
   - 如果用户的输入是问候、询问概念或不需要实际操作 Excel 的请求，请直接用**纯文本**回答，**不要**包含任何代码块。
   
2. **执行任务**：
   - 如果用户要求处理数据、修改 Excel 或计算内容，请务必输出 Python 代码。
   - **代码必须且只能**包含在 Markdown 代码块中，格式如下：
     ```python
     # 你的代码写在这里
     ```
   - 严禁输出代码块以外的解释性文字（除非非常必要），让代码块作为主要回复。

# 代码编写规范 (Production Level)
1. **完整性**：代码必须包含所有必要的 import 语句 (`import pandas as pd`, `import xlwings as xw`, `import os`)。
2. **上下文感知**：用户当前操作的文件路径会包含在消息中，请从中提取并赋值给 `target_file` 变量。
3. **打印输出**：所有处理结果、统计信息必须使用 `print()` 输出，以便在用户界面显示。

# 核心技术规则：智能保存策略 (热更新)
在 Windows 环境下，为了实现“所见即所得”并防止文件锁死 (Permission denied)，**严禁**直接使用 `df.to_excel()` 覆盖原文件。

**请严格按照以下模板结构编写最后的数据写入逻辑**：

```python
import pandas as pd
import xlwings as xw
import os

# ... [这里是你处理数据的逻辑，生成的最终 dataframe 变量名必须为 df] ...

# 【关键】从上下文或硬编码中获取目标路径
# 如果用户没有指定新路径，默认覆盖当前文件
# 注意：Prompt Context 会告诉你当前文件路径，请灵活使用
target_file = r"{file_path_placeholder}" 

try:
    # 1. 尝试连接当前活动的 Excel 实例（热更新模式）
    filename = os.path.basename(target_file)
    
    # 尝试寻找已打开的 workbook
    # 如果文件没打开，xlwings 会抛出异常，自动跳转到 except
    wb = xw.books[filename]
    
    # 2. 如果找到了，直接写入当前活跃界面
    # 激活该工作簿
    wb.activate()
    sheet = wb.sheets.active 
    
    # 清空原有区域，防止旧数据残留 (视情况而定，全量更新时必须清空)
    sheet.clear() 
    
    # 将 DataFrame 写入，默认不带 index (除非用户明确要求保留索引)
    sheet.range('A1').options(index=False).value = df 
    
    print(f"✨ 成功！数据已实时更新到打开的 Excel 窗口：{filename}")

except Exception as e:
    # 3. 如果没打开 Excel，或者连接失败，则降级为写入磁盘
    print(f"👀 未检测到活动的 Excel 窗口，正在保存到磁盘... ({e})")
    try:
        df.to_excel(target_file, index=False)
        print(f"💾 文件已保存到硬盘：{target_file}")
    except Exception as save_error:
         print(f"❌ 保存失败 (文件可能被占用且无法连接): {save_error}")
"#;

/// 读取外置 System Prompt
fn get_system_prompt() -> String {
    let path = Path::new("assets/system_prompt.md");
    match read_to_string(path) {
        Ok(content) => {
            println!("✅ 已加载外部 System Prompt");
            content
        }
        Err(e) => {
            println!("⚠️ 读取 Prompt 失败: {}, 使用内置默认值", e);
            DEFAULT_FALLBACK_PROMPT.to_string()
        }
    }
}

pub async fn call_ai(
    config: &AppConfig,
    user_content: &str,
    context: Option<String>,
) -> Result<String> {
    let profile = config.active_profile();
    let api_key = &profile.api_key;
    let base_url = &profile.base_url;
    let model = &profile.model_id;

    if api_key.is_empty() {
        return Ok("请先在设置中配置 API Key".to_string());
    }

    let client = Client::new();

    // 1. 读取 Prompt
    let mut system_instruction = get_system_prompt();

    // 2. 注入 Context (文件路径、表头)
    if let Some(ctx) = context {
        system_instruction = format!("{}\n\n【Context】\n{}", system_instruction, ctx);
    }

    println!("🤖 请求 AI: {}", model);

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
                { "role": "system", "content": system_instruction },
                { "role": "user", "content": user_content }
            ],
            "temperature": 0.1
        }))
        .send()
        .await?;

    if !response.status().is_success() {
        let error = response.text().await?;
        return Ok(format!("API 请求错误: {}", error));
    }

    let json: Value = response.json().await?;
    let content = json["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("")
        .to_string();

    Ok(content)
}
