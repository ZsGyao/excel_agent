use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Basic View
#[derive(Clone, PartialEq, Copy)]
pub enum View {
    Chat,
    Settings,
}

/// Excel Table Struct
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct TableData {
    pub columns: Vec<String>,
    pub data: Vec<Vec<serde_json::Value>>, // Table unit might be num,str,null.. ,use serde_json to cover all type
}

/// Chat Message
#[derive(Clone, PartialEq, Debug)]
pub struct ChatMessage {
    pub id: usize,
    pub text: String,
    pub is_user: bool,
    pub table: Option<TableData>,
}

/// Python execute result, used to prase JSON that the backend.py return
#[derive(Deserialize, Debug)]
pub struct PyExecResult {
    pub status: String,             // "success" | "error"
    pub message: String,            // The text back to user
    pub preview: Option<TableData>, // Preview Data
}

/// Ai Model Config
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelProfile {
    pub id: String,
    pub name: String, // User set name，like "My DeepSeek"
    pub base_url: String,
    pub api_key: String,
    pub model_id: String, // API param used model name，like "moonshot-v1-8k"
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
        // Default create Moonshot config
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

/// API Request
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<MessageApi>,
}

/// Message Api
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

// AI replay struct define
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AiReply {
    pub reply_type: String, // "code" or "chat"
    pub content: String,    // code or chat text
}
