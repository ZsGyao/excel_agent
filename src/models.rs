use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, PartialEq, Copy)]
pub enum View {
    Chat,
    Settings,
}

#[derive(Clone, PartialEq, Debug)]
pub struct ChatMessage {
    pub id: usize,
    pub text: String,
    pub is_user: bool,
}

// ---------- 配置相关模型 ------------
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelProfile {
    pub id: String,
    pub name: String, // 给用户看的别名，如 "我的 DeepSeek"
    pub base_url: String,
    pub api_key: String,
    pub model_id: String, // API 参数用的模型名，如 "moonshot-v1-8k"
}

impl ModelProfile {
    pub fn new() -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name: "新模型配置".to_string(),
            base_url: "https://api.openai.com/v1".to_string(),
            api_key: "".to_string(),
            model_id: "gpt-3.5-turbo".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AppConfig {
    pub profiles: Vec<ModelProfile>,
    pub active_profile_id: Option<String>,
}

impl Default for AppConfig {
    fn default() -> Self {
        // 默认创建一个 Moonshot 的配置模板
        let default_profile = ModelProfile {
            id: Uuid::new_v4().to_string(),
            name: "Moonshot Kimi".to_string(),
            base_url: "https://api.moonshot.cn/v1".to_string(),
            api_key: "".to_string(),
            model_id: "moonshot-v1-8k".to_string(),
        };

        Self {
            active_profile_id: Some(default_profile.id.clone()),
            profiles: vec![default_profile],
        }
    }
}

// --- API 相关结构 ---
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<MessageApi>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MessageApi {
    pub role: String,
    pub content: String,
}

#[derive(Deserialize, Clone, Debug)]
pub struct ChatResponse {
    pub choices: Vec<Choice>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Choice {
    pub message: MessageApi,
}

// AI 回复的结构化定义
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AiReply {
    pub reply_type: String, // "code" 或 "chat"
    pub content: String,    // 代码内容 或 聊天文本
}
