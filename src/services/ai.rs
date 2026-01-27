use crate::models::{AiReply, ChatRequest, ChatResponse, MessageApi};
use reqwest;
use serde_json;

pub async fn call_ai(
    api_key: String,
    base_url: String,
    model_name: String,
    prompt: String,
    columns: String,
) -> Result<AiReply, String> {
    if api_key.is_empty() {
        return Err("请先在设置中配置 API Key".to_string());
    }

    let client = reqwest::Client::new();

    // 🔥 核心修改：升级 System Prompt，要求返回 JSON
    let system_prompt = format!(
        r#"你是一个 Python Pandas 数据分析专家，同时也是一位助手。
当前处理的 Excel 表格包含列: [{}]。
Dataframe 变量名为 `df`。

请根据用户的输入判断意图，并严格按照以下 JSON 格式返回（不要包含 markdown 代码块标记）：

场景 1：如果用户需要处理数据
{{
  "reply_type": "code",
  "content": "这里写 Python 代码，例如 result = df['Age'].mean()"
}}

场景 2：如果用户只是闲聊或询问非数据问题
{{
  "reply_type": "chat",
  "content": "这里写你的回复文本"
}}

代码要求：
1. 必须修改 `df` 或将结果赋值给 `result`。
2. 只能使用 pandas (pd) 和 numpy (np)。
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
        let raw_content = choice.message.content.clone();

        // 清洗一下可能存在的 markdown 标记 (有些模型不听话，还是会加 ```json)
        let clean_json = raw_content
            .replace("```json", "")
            .replace("```", "")
            .trim()
            .to_string();

        // 尝试解析 JSON
        match serde_json::from_str::<AiReply>(&clean_json) {
            Ok(reply) => Ok(reply),
            Err(_) => {
                // 如果解析失败（模型太笨没返回 JSON），默认当做聊天处理
                // 或者尝试当做代码处理，这里我们保守一点，当做聊天
                Ok(AiReply {
                    reply_type: "chat".to_string(),
                    content: raw_content, // 原样返回
                })
            }
        }
    } else {
        Err("AI 未返回内容".into())
    }
}
