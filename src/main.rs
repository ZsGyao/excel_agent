#![allow(non_snake_case)]

mod components;
mod models;
mod services;

use dioxus::desktop::{Config, LogicalSize, WindowBuilder};
use dioxus::html::HasFileData;
use dioxus::prelude::*;

use models::{ChatMessage, View};
// ✅ 修复点：明确引入 load_config
use crate::services::config::load_config;
use components::{
    chat_view::ChatView, input_area::InputArea, settings::Settings, sidebar::Sidebar,
};

fn main() {
    dioxus_logger::init(tracing::Level::INFO).expect("failed to init logger");

    services::python::init_python_env();

    let config = Config::new()
        .with_custom_head(r#"<link rel="stylesheet" href="style.css">"#.to_string())
        .with_window(
            WindowBuilder::new()
                .with_title("Excel AI Agent")
                .with_inner_size(LogicalSize::new(900.0, 700.0))
                .with_resizable(true),
        );

    LaunchBuilder::desktop().with_cfg(config).launch(App);
}

#[component]
fn App() -> Element {
    let current_view = use_signal(|| View::Chat);
    let mut messages = use_signal(|| {
        vec![ChatMessage {
            id: 0,
            text: "👋 嗨！把 Excel 拖进来，然后去设置里配一下 API Key。".into(),
            is_user: false,
        }]
    });

    // ✅ 修复点：调用 load_config() 加载配置
    let config = use_signal(|| load_config());

    let mut last_file_path = use_signal(|| String::new());
    let mut is_dragging = use_signal(|| false);
    let is_loading = use_signal(|| false);

    rsx! {
        document::Stylesheet { href: asset!("/assets/main.css") }

        div {
            class: "app-container",
            ondragover: move |evt| { evt.prevent_default(); if !is_dragging() { is_dragging.set(true); } },
            ondragleave: move |evt| { evt.prevent_default(); is_dragging.set(false); },
            ondrop: move |evt| {
                evt.prevent_default();
                is_dragging.set(false);
                let files = evt.data().files();
                if let Some(first_file) = files.first() {
                    let file_name = first_file.name();
                    let current_dir = std::env::current_dir().unwrap();
                    let full_path = current_dir.join(&file_name).to_str().unwrap().to_string();

                    last_file_path.set(full_path.clone());

                    let new_id = messages.read().len();
                    messages.write().push(ChatMessage {
                        id: new_id,
                        text: format!("📂 已加载: {}", file_name),
                        is_user: false
                    });
                }
            },

            Sidebar { current_view: current_view }

            div { class: "content-area",
                if is_dragging() { div { class: "drag-overlay", "📂 投喂 Excel！" } }

                if is_loading() {
                    div {
                        style: "position: absolute; top: 10px; right: 10px; background: #ff69b4; color: white; padding: 5px 10px; border-radius: 12px; font-size: 12px; z-index: 999;",
                        "🧠 AI 思考中..."
                    }
                }

                if current_view() == View::Chat {
                    ChatView { messages: messages }
                    InputArea {
                        messages: messages,
                        last_file_path: last_file_path,
                        is_loading: is_loading,
                        config: config,
                    }
                } else if current_view() == View::Settings {
                    Settings { config: config }
                }
            }
        }
    }
}
