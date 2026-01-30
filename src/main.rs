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
use crate::models::WindowMode;
use crate::services::config::load_config;
use components::{
    chat_view::ChatView, input_area::InputArea, settings::Settings, sidebar::Sidebar,
};
use models::{ChatMessage, View};

// 引入 Windows API 获取工作区 (Work Area)
#[cfg(target_os = "windows")]
use windows_sys::Win32::Foundation::RECT;
#[cfg(target_os = "windows")]
use windows_sys::Win32::UI::WindowsAndMessaging::{SystemParametersInfoW, SPI_GETWORKAREA};

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
        .with_inner_size(LogicalSize::new(140.0, 56.0)) // Init is Float ball widget
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

// 🔥 辅助函数：获取屏幕可用工作区（排除任务栏）
// 返回值：(可用宽度, 可用高度, 左上角X, 左上角Y) 都是物理像素
#[cfg(target_os = "windows")]
fn get_work_area_rect() -> (i32, i32, i32, i32) {
    unsafe {
        let mut rect = std::mem::zeroed::<RECT>();
        // SPI_GETWORKAREA 获取主显示器的工作区
        if SystemParametersInfoW(SPI_GETWORKAREA, 0, &mut rect as *mut _ as *mut _, 0) != 0 {
            let width = rect.right - rect.left;
            let height = rect.bottom - rect.top;
            return (width, height, rect.left, rect.top);
        }
    }
    // 获取失败兜底：返回一个默认大尺寸
    (1920, 1080, 0, 0)
}

#[component]
fn App() -> Element {
    let window = dioxus::desktop::use_window();
    let mut window_mode = use_signal(|| WindowMode::Widget);

    // 记忆胶囊展开前的位置
    // 使用 Option 是为了处理首次启动还没有记录的情况
    let mut last_widget_pos = use_signal(|| None::<PhysicalPosition<i32>>);

    // 尺寸常量
    const CAPSULE_W: f64 = 140.0;
    const CAPSULE_H: f64 = 56.0;
    const CARD_W: f64 = 480.0;
    const MARGIN: f64 = 60.0;

    // 初始化：强制把胶囊放到屏幕右边缘 (垂直居中)
    let window_init = window.clone();
    use_effect(move || {
        if let Some(monitor) = window_init.current_monitor() {
            let scale = monitor.scale_factor();
            let (work_w_phys, work_h_phys, _, work_y_phys) = get_work_area_rect();

            // 垂直居中初始化
            let center_y =
                (work_y_phys as f64 / scale) + (work_h_phys as f64 / scale - CAPSULE_H) / 2.0;
            let default_x = (work_w_phys as f64 / scale) - CAPSULE_W;

            window_init.set_outer_position(LogicalPosition::new(default_x, center_y));
            // 记录初始位置
            let phys_x = (default_x * scale).round() as i32;
            let phys_y = (center_y * scale).round() as i32;
            last_widget_pos.set(Some(PhysicalPosition::new(phys_x, phys_y)));

            // 强制聚焦，激活窗口交互
            window_init.set_focus();
        }
    });

    // Dynamically adjust window size based on changes in monitoring mode
    let window_effect = window.clone();
    use_effect(move || {
        // 获取当前屏幕信息
        let monitor_opt = window_effect.current_monitor();
        if monitor_opt.is_none() {
            return;
        }
        let monitor = monitor_opt.unwrap();
        let scale = monitor.scale_factor();

        // 获取工作区数据 (排除任务栏)
        let (work_w_phys, work_h_phys, work_x_phys, work_y_phys) = get_work_area_rect();
        let work_w = work_w_phys as f64 / scale; // 逻辑宽度
        let work_h = work_h_phys as f64 / scale; // 逻辑高度
        let work_top = work_y_phys as f64 / scale; // 工作区顶边 (通常是0，但如果任务栏在上面则不是)

        match window_mode() {
            WindowMode::Widget => {
                // === 收起回胶囊 ===
                window_effect.set_inner_size(LogicalSize::new(CAPSULE_W, CAPSULE_H));
                window_effect.set_always_on_top(true);

                if let Some(pos) = last_widget_pos() {
                    let logic_x = pos.x as f64 / scale;
                    let logic_y = pos.y as f64 / scale;
                    window_effect.set_outer_position(LogicalPosition::new(logic_x, logic_y));
                } else {
                    // 兜底回右侧居中
                    let center_y = work_top + (work_h - CAPSULE_H) / 2.0;
                    let default_x = (work_w_phys as f64 / scale) - CAPSULE_W;
                    window_effect.set_outer_position(LogicalPosition::new(default_x, center_y));
                }
                window_effect.set_focus();
            }
            WindowMode::Main => {
                // === 展开 ===
                if let Ok(current_pos) = window_effect.outer_position() {
                    last_widget_pos.set(Some(current_pos));
                    let current_x_logical = current_pos.x as f64 / scale;

                    // 🔥 核心逻辑：高度自动填满
                    // 高度 = 工作区高度 - 上下边距
                    let target_h = work_h - (MARGIN * 2.0);
                    // Y坐标 = 工作区顶部 + 边距
                    let target_y = work_top + MARGIN;

                    // X坐标：判断靠左还是靠右
                    let screen_center_x = (work_x_phys as f64 / scale) + (work_w / 2.0);
                    let target_x = if current_x_logical > screen_center_x {
                        // 靠右
                        (work_w_phys as f64 / scale) - CARD_W - MARGIN
                    } else {
                        // 靠左
                        (work_x_phys as f64 / scale) + MARGIN
                    };

                    window_effect.set_outer_position(LogicalPosition::new(target_x, target_y));
                    // 🔥 设置动态计算出的高度
                    window_effect.set_inner_size(LogicalSize::new(CARD_W, target_h));
                }

                window_effect.set_focus();
                window_effect.set_always_on_top(true);
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
