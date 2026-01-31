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
use components::{chat_view::ChatView, input_area::InputArea, settings::Settings};
use models::{ChatMessage, View};

#[cfg(target_os = "windows")]
use windows_sys::Win32::Foundation::RECT;
#[cfg(target_os = "windows")]
use windows_sys::Win32::UI::WindowsAndMessaging::{SystemParametersInfoW, SPI_GETWORKAREA};

fn main() {
    dioxus_logger::init(tracing::Level::INFO).expect("failed to init logger");
    services::python::init_python_env();

    let icon_path = "assets/icon.png";
    let icon = load_icon(Path::new(icon_path));

    let _tray = match icon {
        Ok(i) => Some(Box::leak(Box::new(
            TrayIconBuilder::new()
                .with_tooltip("Excel AI Agent")
                .with_icon(i)
                .build()
                .unwrap(),
        ))),
        Err(_) => {
            println!("⚠️ 警告：找不到 assets/icon.png，托盘图标加载失败");
            None
        }
    };

    let window_builder = WindowBuilder::new()
        .with_title("Excel Agent")
        .with_inner_size(LogicalSize::new(130.0, 160.0))
        .with_decorations(false)
        .with_transparent(true)
        .with_visible(false) // 初始隐藏，防止白屏
        .with_undecorated_shadow(false)
        .with_skip_taskbar(true)
        .with_always_on_top(true);

    let config = Config::new().with_window(window_builder);

    LaunchBuilder::desktop().with_cfg(config).launch(App);
}

fn load_icon(path: &Path) -> anyhow::Result<Icon> {
    let (icon_rgba, icon_width, icon_height) = {
        let image = image::open(path)?.into_rgba8();
        let (width, height) = image.dimensions();
        let rgba = image.into_raw();
        (rgba, width, height)
    };
    Ok(Icon::from_rgba(icon_rgba, icon_width, icon_height)?)
}

#[cfg(target_os = "windows")]
fn get_work_area_rect() -> (i32, i32, i32, i32) {
    unsafe {
        let mut rect = std::mem::zeroed::<RECT>();
        if SystemParametersInfoW(SPI_GETWORKAREA, 0, &mut rect as *mut _ as *mut _, 0) != 0 {
            let width = rect.right - rect.left;
            let height = rect.bottom - rect.top;
            return (width, height, rect.left, rect.top);
        }
    }
    (1920, 1080, 0, 0)
}

#[component]
fn App() -> Element {
    let window = dioxus::desktop::use_window();
    let mut window_mode = use_signal(|| WindowMode::Widget);
    let mut last_widget_pos = use_signal(|| None::<PhysicalPosition<i32>>);

    const CAPSULE_W: f64 = 130.0;
    const CAPSULE_H: f64 = 160.0;
    const CARD_W: f64 = 480.0;
    const SETTINGS_W: f64 = 750.0;
    const SETTINGS_H: f64 = 550.0;
    const MARGIN: f64 = 60.0;

    // 初始化定位
    let window_init = window.clone();
    use_effect(move || {
        if let Some(monitor) = window_init.current_monitor() {
            let scale = monitor.scale_factor();
            let (work_w_phys, work_h_phys, _, work_y_phys) = get_work_area_rect();

            let visual_center_offset = 25.0;
            let center_y = (work_y_phys as f64 / scale) + (work_h_phys as f64 / scale / 2.0)
                - visual_center_offset;
            let default_x = (work_w_phys as f64 / scale) - CAPSULE_W;

            window_init.set_outer_position(LogicalPosition::new(default_x, center_y));

            let phys_x = (default_x * scale).round() as i32;
            let phys_y = (center_y * scale).round() as i32;
            last_widget_pos.set(Some(PhysicalPosition::new(phys_x, phys_y)));

            window_init.set_visible(true);
            window_init.set_focus();
        }
    });

    // 核心：监听模式变化，调整窗口物理属性
    let window_effect = window.clone();
    use_effect(move || {
        // 读取信号，建立依赖
        let mode = window_mode();

        let monitor_opt = window_effect.current_monitor();
        if monitor_opt.is_none() {
            return;
        }
        let monitor = monitor_opt.unwrap();
        let scale = monitor.scale_factor();
        let (work_w_phys, work_h_phys, work_x_phys, work_y_phys) = get_work_area_rect();
        let work_w = work_w_phys as f64 / scale;
        let work_h = work_h_phys as f64 / scale;
        let work_top = work_y_phys as f64 / scale;

        // 再次强制隐藏，确保万无一失
        window_effect.set_visible(false);

        match mode {
            WindowMode::Widget => {
                window_effect.set_inner_size(LogicalSize::new(CAPSULE_W, CAPSULE_H));
                window_effect.set_always_on_top(true);

                if let Some(pos) = last_widget_pos() {
                    let logic_x = pos.x as f64 / scale;
                    let logic_y = pos.y as f64 / scale;
                    window_effect.set_outer_position(LogicalPosition::new(logic_x, logic_y));
                } else {
                    let center_y = work_top + (work_h - CAPSULE_H) / 2.0;
                    let default_x = (work_w_phys as f64 / scale) - CAPSULE_W;
                    window_effect.set_outer_position(LogicalPosition::new(default_x, center_y));
                }
            }
            WindowMode::Main => {
                if let Ok(current_pos) = window_effect.outer_position() {
                    if window_effect.inner_size().width < 200 {
                        last_widget_pos.set(Some(current_pos));
                    }
                    let anchor_pos = last_widget_pos().unwrap_or(current_pos);
                    let anchor_x = anchor_pos.x as f64 / scale;
                    let target_h = work_h - (MARGIN * 2.0);
                    let target_y = work_top + MARGIN;
                    let screen_center_x = (work_x_phys as f64 / scale) + (work_w / 2.0);
                    let target_x = if anchor_x > screen_center_x {
                        (work_w_phys as f64 / scale) - CARD_W - MARGIN
                    } else {
                        (work_x_phys as f64 / scale) + MARGIN
                    };
                    window_effect.set_outer_position(LogicalPosition::new(target_x, target_y));
                    window_effect.set_inner_size(LogicalSize::new(CARD_W, target_h));
                }
                window_effect.set_always_on_top(true);
            }
            WindowMode::Settings => {
                let center_x = (work_x_phys as f64 / scale) + (work_w - SETTINGS_W) / 2.0;
                let center_y = work_top + (work_h - SETTINGS_H) / 2.0;
                window_effect.set_inner_size(LogicalSize::new(SETTINGS_W, SETTINGS_H));
                window_effect.set_outer_position(LogicalPosition::new(center_x, center_y));
                window_effect.set_always_on_top(false);
            }
        }

        // 延迟显示：这是防闪烁的第二道防线
        let window_show = window_effect.clone();
        spawn(async move {
            tokio::time::sleep(Duration::from_millis(200)).await;
            window_show.set_visible(true);
            window_show.set_focus();
        });
    });

    // 修复托盘逻辑报错
    let window_tray = window.clone();
    use_future(move || {
        let window = window_tray.clone(); // 🔥 修复 E0507: 在这里 clone
        async move {
            let receiver = TrayIconEvent::receiver();
            loop {
                if let Ok(event) = receiver.try_recv() {
                    if let TrayIconEvent::Click { .. } = event {
                        window.set_visible(false);
                        window_mode.set(WindowMode::Main);
                    }
                }
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

    // 为按钮事件准备的 Window Clone
    let window_close_settings = window.clone();
    let window_to_settings = window.clone();
    let window_to_widget = window.clone();

    rsx! {
        document::Stylesheet { href: asset!("/assets/main.css") }

        if window_mode() == WindowMode::Widget {
            DockCapsule { window_mode, messages, last_file_path }
        } else if window_mode() == WindowMode::Settings {
            div {
                class: "window-frame settings-panel",
                oncontextmenu: move |evt| evt.prevent_default(),
                Settings {
                    config,
                    // 🔥 策略核心：先隐藏 -> 等50ms -> 再切换状态
                    on_close: move |_| {
                        let win = window_close_settings.clone();
                        win.set_visible(false); // 1. 马上消失
                        spawn(async move {
                            tokio::time::sleep(Duration::from_millis(50)).await; // 2. 给系统喘息时间
                            window_mode.set(WindowMode::Widget); // 3. 切换状态（此时窗口是隐藏的）
                        });
                    },
                }
            }
        } else {
            div {
                class: "window-frame main-panel",
                oncontextmenu: move |evt| evt.prevent_default(),

                div { class: "panel-header",
                    div { class: "title-text", "Excel AI Agent" }
                    // 切换到设置
                    div {
                        class: "icon-btn",
                        title: "设置",
                        onclick: move |_| {
                            let win = window_to_settings.clone();
                            win.set_visible(false);
                            spawn(async move {
                                tokio::time::sleep(Duration::from_millis(50)).await;
                                window_mode.set(WindowMode::Settings);
                            });
                        },
                        "⚙️"
                    }
                    // 最小化到 Widget
                    div {
                        style: "cursor: pointer; padding: 5px;",
                        onclick: move |_| {
                            let win = window_to_widget.clone();
                            win.set_visible(false);
                            spawn(async move {
                                tokio::time::sleep(Duration::from_millis(50)).await;
                                window_mode.set(WindowMode::Widget);
                            });
                        },
                        "⏬"
                    }
                }

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
                        if let Some(first_file) = files.first() {}
                    },

                    div { class: "content-area",
                        if is_dragging() {
                            div { class: "drag-overlay", "📂 投喂 Excel！" }
                        }
                        ChatView { messages, last_file_path }
                        InputArea {
                            messages,
                            last_file_path,
                            is_loading,
                            config,
                        }
                    }
                }
            }
        }
    }
}
