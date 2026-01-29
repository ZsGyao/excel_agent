#![allow(non_snake_case)]

mod components;
mod models;
mod services;

use std::path::Path;
use std::time::Duration;

use dioxus::desktop::tao::platform::windows::WindowBuilderExtWindows;
use dioxus::desktop::trayicon::{Icon, TrayIconBuilder, TrayIconEvent};
use dioxus::desktop::{Config, LogicalPosition, LogicalSize, WindowBuilder};
use dioxus::html::HasFileData;
use dioxus::prelude::*;

use crate::components::title_bar::TitleBar;
use crate::components::widget_ball::WidgetBall;
use crate::models::WindowMode;
use crate::services::config::load_config;
use components::{
    chat_view::ChatView, input_area::InputArea, settings::Settings, sidebar::Sidebar,
};
use models::{ChatMessage, View};

fn main() {
    dioxus_logger::init(tracing::Level::INFO).expect("failed to init logger");
    services::python::init_python_env();

    let icon_path = "assets/icon.png";
    let icon = load_icon(Path::new(icon_path));

    // Create system tray, use Box::leak to keep trap alive during program runtime
    let _tray = match icon {
        Ok(i) => {
            Some(Box::leak(Box::new(
                TrayIconBuilder::new()
                    .with_tooltip("Excel AI Agent") // Show text when mouse hover
                    .with_icon(i)
                    .build()
                    .unwrap(),
            )))
        }
        Err(_) => {
            println!("⚠️ 警告：找不到 assets/icon.png，托盘图标加载失败");
            None
        }
    };

    // Create Window builder and config
    let window_builder = WindowBuilder::new()
        .with_title("Excel Agent")
        .with_inner_size(LogicalSize::new(80.0, 80.0)) // Init is Float ball widget
        .with_decorations(false)
        .with_transparent(true)
        .with_visible(true)
        .with_undecorated_shadow(false)
        .with_skip_taskbar(true) // Hide from the taskbar
        .with_always_on_top(true); // Float ball always on the top

    let config = Config::new().with_window(window_builder);

    LaunchBuilder::desktop().with_cfg(config).launch(App);
}

/// Read Png and transform to Icon
fn load_icon(path: &Path) -> anyhow::Result<Icon> {
    let (icon_rgba, icon_width, icon_height) = {
        let image = image::open(path)?.into_rgba8();
        let (width, height) = image.dimensions();
        let rgba = image.into_raw();
        (rgba, width, height)
    };
    Ok(Icon::from_rgba(icon_rgba, icon_width, icon_height)?)
}

#[component]
fn App() -> Element {
    let window = dioxus::desktop::use_window();
    let mut window_mode = use_signal(|| WindowMode::Widget);
    let window_for_effect = window.clone();
    // Dynamically adjust window size based on changes in monitoring mode
    use_effect(move || {
        match window_mode() {
            WindowMode::Widget => {
                // 初始状态：小胶囊
                // 宽度 40 (Logo + padding), 高度 60
                window_for_effect.set_inner_size(LogicalSize::new(40.0, 60.0));
                window_for_effect.set_always_on_top(true);

                // TODO: 这里其实需要记忆上次是 Left 还是 Right，并恢复位置
                // 暂时先让用户自己拖回去，或者默认吸附右边
            }
            WindowMode::Main => {
                // 展开状态：长条面板 (手机比例)
                let panel_width = 400.0;
                let panel_height = 700.0;

                // 获取屏幕宽度，判断当前在哪边，决定面板弹出的 X 坐标
                let monitor = window_for_effect.current_monitor().unwrap();
                let screen_width = monitor.size().width as f64 / monitor.scale_factor();
                let win_pos = window_for_effect.outer_position().unwrap();
                let win_x = win_pos.x as f64 / monitor.scale_factor();

                // 如果当前在左半屏 -> 面板贴左 (x=0)
                // 如果当前在右半屏 -> 面板贴右 (x = Screen - Panel_Width)
                let new_x = if win_x < screen_width / 2.0 {
                    0.0
                } else {
                    screen_width - panel_width
                };

                // 设置位置和大小
                window_for_effect.set_outer_position(LogicalPosition::new(
                    new_x,
                    win_pos.y as f64 / monitor.scale_factor(),
                ));
                window_for_effect.set_inner_size(LogicalSize::new(panel_width, panel_height));

                window_for_effect.set_focus();
                window_for_effect.set_always_on_top(true); // 保持置顶
            }
        }
    });

    // Listen tray click envet, Use use_future start async task
    use_future(move || {
        // Get window handle to control show/hide
        let window = window.clone();
        async move {
            let receiver = TrayIconEvent::receiver();

            loop {
                // Use try_recv to check event unblocking
                if let Ok(event) = receiver.try_recv() {
                    // if is click event
                    if let TrayIconEvent::Click { .. } = event {
                        println!("托盘图标被点击！");
                        window.set_visible(true);
                        window.set_focus();
                        window_mode.set(WindowMode::Main);
                    }
                }
                // Sleep a while, avoid loop use 100% CPU
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
        }
    });

    let current_view = use_signal(|| View::Chat);
    let mut messages = use_signal(|| {
        vec![ChatMessage {
            id: 0,
            text: "👋 嗨！把 Excel 拖进来，然后去设置里配一下 API Key。".into(),
            is_user: false,
            table: None,
            temp_id: None,
            status: models::ActionStatus::None,
            image: None,
        }]
    });

    let config = use_signal(|| load_config());

    let mut last_file_path = use_signal(|| String::new());
    let mut is_dragging = use_signal(|| false);
    let is_loading = use_signal(|| false);

    rsx! {
        document::Stylesheet { href: asset!("/assets/main.css") }

        if window_mode() == WindowMode::Widget {
            WidgetBall {
                window_mode,
                is_dragging,
                messages,
                last_file_path,
            }
        } else {
            div { class: "window-frame",
                // // 这里的 TitleBar 需要稍微改一下，最小化按钮变成“收起到悬浮球” todo
                TitleBar {}

                div {
                    class: "app-container",
                    ondragover: move |evt| {
                        evt.prevent_default();
                        if !is_dragging() {
                            is_dragging.set(true);
                        }
                    },
                    ondragleave: move |evt| {
                        evt.prevent_default();
                        is_dragging.set(false);
                    },
                    ondrop: move |evt| {
                        evt.prevent_default();
                        is_dragging.set(false);
                        let files = evt.data().files();
                        if let Some(first_file) = files.first() {
                            // todo: Set the actually file path, now just support project dir
                            let file_name = first_file.name();
                            let current_dir = std::env::current_dir().unwrap();
                            let full_path = current_dir.join(&file_name).to_str().unwrap().to_string();

                            last_file_path.set(full_path.clone());

                            let new_id = messages.read().len();
                            messages
                                .write()
                                .push(ChatMessage {
                                    id: new_id,
                                    text: format!("📂 已加载: {}", file_name),
                                    is_user: false,
                                    table: None,
                                    temp_id: None,
                                    status: models::ActionStatus::None,
                                    image: None,
                                });
                        }
                    },
                    div {
                        style: "position: absolute; top: 10px; right: 50px; cursor: pointer; z-index: 9999;",
                        onclick: move |_| window_mode.set(WindowMode::Widget),
                        "⏬"
                    }

                    Sidebar { current_view }

                    div { class: "content-area",
                        if is_dragging() {
                            div { class: "drag-overlay", "📂 投喂 Excel！" }
                        }

                        if is_loading() {
                            div { class: "loading-badge", "🧠 AI 思考中..." }
                        }

                        if current_view() == View::Chat {
                            ChatView { messages, last_file_path }
                            InputArea {
                                messages,
                                last_file_path,
                                is_loading,
                                config,
                            }
                        } else if current_view() == View::Settings {
                            Settings { config }
                        }
                    }
                }
            }
        }

    }
}
