// src/store/app_state.rs
use dioxus::desktop::wry::dpi::PhysicalPosition;
use dioxus::prelude::*;
use std::collections::HashMap;

use crate::models::AppConfig;
use crate::models::{ChatMessage, PendingImport, WindowMode};
use crate::services::ai::generate_dehydrated_schema_json;
use crate::services::config::load_config;
use crate::services::excel_engine::FileSchema;

/// 整个 App 的唯一真理之源 (Single Source of Truth)
#[derive(Clone, Copy)]
pub struct AppState {
    pub window_mode: Signal<WindowMode>,
    pub last_widget_pos: Signal<Option<PhysicalPosition<i32>>>,
    pub messages: Signal<Vec<ChatMessage>>,
    pub config: Signal<AppConfig>,
    pub active_files: Signal<Vec<String>>,
    pub is_loading: Signal<bool>,
    pub error_fix_signal: Signal<Option<String>>,
    pub retry_count: Signal<i32>,
    pub pending_import: Signal<Option<PendingImport>>,
    pub global_schemas: Signal<HashMap<String, FileSchema>>,
    pub ai_context_json: Memo<String>,
}

/// 在 App 根节点调用，初始化并注入全局状态
pub fn use_init_app_state() -> AppState {
    // 先把依赖初始化的存起来
    let mut global_schemas = use_signal(|| HashMap::<String, FileSchema>::new());

    let state = AppState {
        window_mode: use_signal(|| WindowMode::Widget),
        last_widget_pos: use_signal(|| None::<PhysicalPosition<i32>>),
        messages: use_signal(|| vec![ChatMessage::new(0, "👋 嗨！把 Excel 拖进来开始吧。", false)]),
        config: use_signal(|| load_config()),
        active_files: use_signal(|| Vec::<String>::new()),
        is_loading: use_signal(|| false),
        error_fix_signal: use_signal(|| None::<String>),
        retry_count: use_signal(|| 0),
        pending_import: use_signal(|| None::<PendingImport>),
        global_schemas,
        // Memo 依赖 global_schemas
        ai_context_json: use_memo(move || {
            let json_str = generate_dehydrated_schema_json(&global_schemas.read());
            if json_str.len() > 2 {
                debug!("\n ============ Excel Prompt JSON ===============\n");
                debug!("{}", json_str);
                debug!("\n ==============================================\n");
            }
            json_str
        }),
    };

    // 将组装好的 State 放入全局上下文
    use_context_provider(|| state);
    state
}

/// 在子组件中调用，随时随地获取状态！不用再传 Props！
pub fn use_app_state() -> AppState {
    use_context::<AppState>()
}
