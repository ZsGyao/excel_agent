use crate::models::{ChatRequest, ChatResponse, MessageApi};
use reqwest;

pub async fn call_ai(
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
