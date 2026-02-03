use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 窗口显示模式
#[derive(Clone, PartialEq, Debug, Copy)]
pub enum WindowMode {
    /// 桌面小部件模式（胶囊悬浮窗）
    Widget,
    /// 主聊天界面
    Main,
    /// 设置界面
    Settings,
}

/// 操作执行的状态机
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum ActionStatus {
    /// 初始状态，无操作
    None,
    /// 等待 AI 响应中
    Loading,
    /// 🔥 关键状态：AI 生成了代码，等待用户点击“执行”
    WaitingConfirmation,
    /// Python 代码正在后台执行
    Running,
    /// 执行成功
    Success,
    /// 执行出错，包含错误信息
    Error(String),
    /// 用户点击了“取消”
    Cancelled,
    /// 用户点击了“撤销”，已恢复备份
    Undone,
}

/// 聊天消息结构体
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct ChatMessage {
    pub id: usize,
    pub text: String,
    pub is_user: bool,
    /// 可选：如果是数据消息，包含 HTML 表格
    pub table: Option<String>,
    pub temp_id: Option<String>,
    /// 当前消息的状态
    pub status: ActionStatus,
    /// 可选：图片路径
    pub image: Option<String>,

    /// 待执行的 Python 代码 (仅当 status == WaitingConfirmation 时有效)
    pub pending_code: Option<String>,
    /// 备份路径映射 (仅当 status == Success 时有效)
    /// 支持多文件同时回滚 Vec<(原文件路径，备份文件路径)>
    pub backup_paths: Option<Vec<(String, String)>>,
}

impl ChatMessage {
    /// 创建一条普通消息
    ///
    /// # 参数
    /// * `id` - 消息唯一 ID
    /// * `text` - 消息文本
    /// * `is_user` - 是否为用户发送
    pub fn new(id: usize, text: impl Into<String>, is_user: bool) -> Self {
        Self {
            id,
            text: text.into(),
            is_user,
            table: None,
            temp_id: None,
            status: ActionStatus::None,
            image: None,
            pending_code: None,
            backup_paths: None,
        }
    }

    /// 创建一条 AI "思考中" 的占位消息
    pub fn loading(id: usize) -> Self {
        Self {
            id,
            text: "正在思考...".into(),
            is_user: false,
            table: None,
            temp_id: None,
            status: ActionStatus::Loading,
            image: None,
            pending_code: None,
            backup_paths: None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct ModelProfile {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub model_id: String,
    pub api_key: String,
}

impl ModelProfile {
    pub fn new() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: "New Profile".into(),
            base_url: "https://api.openai.com/v1".into(),
            model_id: "gpt-3.5-turbo".into(),
            api_key: "".into(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct AppConfig {
    pub profiles: Vec<ModelProfile>,
    pub active_profile_id: Option<String>,
    pub custom_prompt: Option<String>,
}

impl AppConfig {
    pub fn default() -> Self {
        let default_profile = ModelProfile {
            id: "default".into(),
            name: "Moonshot".into(),
            base_url: "https://api.moonshot.cn/v1".into(),
            model_id: "moonshot-v1-8k".into(),
            api_key: "".into(),
        };
        Self {
            profiles: vec![default_profile.clone()],
            active_profile_id: Some("default".into()),
            custom_prompt: None,
        }
    }

    /// 获取当前激活的模型配置
    pub fn active_profile(&self) -> ModelProfile {
        if let Some(id) = &self.active_profile_id {
            if let Some(p) = self.profiles.iter().find(|p| &p.id == id) {
                return p.clone();
            }
        }
        self.profiles
            .first()
            .cloned()
            .unwrap_or_else(|| ModelProfile::new())
    }
}
