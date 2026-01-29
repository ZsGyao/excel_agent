#![allow(non_snake_case)]

mod components;
mod models;
mod services;

use std::path::Path;
use std::time::Duration;

use dioxus::desktop::tao::platform::windows::WindowBuilderExtWindows;
use dioxus::desktop::trayicon::{Icon, TrayIconBuilder, TrayIconEvent};
use dioxus::desktop::wry::dpi::PhysicalPosition;
use dioxus::desktop::{Config, LogicalPosition, LogicalSize, WindowBuilder};
use dioxus::html::HasFileData;
use dioxus::prelude::*;

use crate::components::dock_capsule::DockCapsule;
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
        .with_inner_size(LogicalSize::new(48.0, 56.0)) // Init is Float ball widget
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

    // 初始化：强制把胶囊放到屏幕右边缘 (垂直居中)
    let window_init = window.clone();
    use_effect(move || {
        if let Some(monitor) = window_init.current_monitor() {
            let scale = monitor.scale_factor();
            let screen_w = monitor.size().width as f64 / scale;
            let screen_h = monitor.size().height as f64 / scale;

            // 贴右边
            window_init
                .set_outer_position(LogicalPosition::new(screen_w - 48.0, screen_h / 2.0 - 28.0));
        }
    });

    // Dynamically adjust window size based on changes in monitoring mode
    let window_for_effect = window.clone();
    use_effect(move || {
        match window_mode() {
            WindowMode::Widget => {
                // 初始状态：小胶囊
                // 宽度 40 (Logo + padding), 高度 60
                window_for_effect.set_inner_size(LogicalSize::new(48.0, 56.0));
                window_for_effect.set_always_on_top(true);

                // TODO: 这里其实需要记忆上次是 Left 还是 Right，并恢复位置
                // 暂时先让用户自己拖回去，或者默认吸附右边
            }
            WindowMode::Main => {
                let panel_w = 380.0;

                if let Some(monitor) = window_for_effect.current_monitor() {
                    let scale = monitor.scale_factor();
                    let screen_w = monitor.size().width as f64 / scale;
                    let screen_h = monitor.size().height as f64 / scale;

                    // 获取当前位置
                    let pos = window_for_effect
                        .outer_position()
                        .unwrap_or(PhysicalPosition::new(0, 0));
                    let x = pos.x as f64 / scale;

                    // 判断在那边
                    let new_x = if x < screen_w / 2.0 {
                        0.0
                    } else {
                        screen_w - panel_w
                    };

                    // 🔥 强制：顶天立地，贴边
                    window_for_effect.set_outer_position(LogicalPosition::new(new_x, 0.0));
                    window_for_effect.set_inner_size(LogicalSize::new(panel_w, screen_h));
                }
                window_for_effect.set_focus();
                window_for_effect.set_always_on_top(true);
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
            DockCapsule { window_mode, messages, last_file_path }
        } else {
            // Main 面板
            div { class: "window-frame main-panel",
                // Header
                div { class: "panel-header",
                    div { class: "title-text", "Excel AI Agent" }
                    // 只是收起，不关闭
                    div {
                        style: "cursor: pointer; padding: 5px;",
                        onclick: move |_| window_mode.set(WindowMode::Widget),
                        "⏬"
                    }
                }

                div {
                    class: "app-container",
                    // 拖拽文件逻辑 (保持不变)
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
                        if let Some(first_file) = files.first() {} // ... 之前的逻辑 ...
                    },

                    Sidebar { current_view }

                    div { class: "content-area",
                        if is_dragging() {
                            div { class: "drag-overlay", "📂 投喂 Excel！" }
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
