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
use crate::models::{ActionStatus, WindowMode};
use crate::services::config::load_config;
use crate::services::python::{backup_file, restore_file, run_python_code};
use components::{chat_view::ChatView, input_area::InputArea, settings::Settings};
use models::ChatMessage;

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

    // 🔥 恢复默认行为：不需要在这里 set_visible(false)
    // 除非你真的想防止启动那一下白屏，否则 true 体验更好
    let window_builder = WindowBuilder::new()
        .with_title("Excel Agent")
        .with_inner_size(LogicalSize::new(130.0, 160.0))
        .with_decorations(false)
        .with_transparent(true)
        .with_visible(true)
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

// 🔥🔥🔥 核心：Windows 原子操作函数 🔥🔥🔥
// 这个函数会同时修改位置和大小，操作系统保证这发生在同一帧
#[cfg(target_os = "windows")]
fn atomic_update_window(
    window: &dioxus::desktop::DesktopContext,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    always_on_top: bool,
) {
    // 1. 获取底层 HWND 句柄

    use raw_window_handle::HasWindowHandle;
    let hwnd = if let Ok(handle) = window.window_handle() {
        use raw_window_handle::RawWindowHandle;

        if let RawWindowHandle::Win32(win32_handle) = handle.as_raw() {
            use windows_sys::Win32::Foundation::HWND;

            Some(win32_handle.hwnd.get() as HWND)
        } else {
            None
        }
    } else {
        None
    };

    if let Some(hwnd) = hwnd {
        // 2. 调用 SetWindowPos 原子更新
        // SWP_NOACTIVATE: 不自动激活窗口（防止抢焦点）
        // SWP_NOZORDER: 保持当前的 Z 轴顺序（置顶状态由 Dioxus 管理，或者我们自己管理 ）
        unsafe {
            use windows_sys::Win32::UI::WindowsAndMessaging::{
                SetWindowPos, SWP_NOACTIVATE, SWP_NOZORDER,
            };

            SetWindowPos(
                hwnd,
                std::ptr::null_mut(), // 这里不改变 Z-order，除非我们需要强制置顶
                x,
                y,
                w,
                h,
                SWP_NOACTIVATE | SWP_NOZORDER,
            );
        }
    } else {
        // 兜底：如果获取不到句柄，回退到 Dioxus 的方法

        use dioxus::desktop::wry::dpi::PhysicalSize;
        window.set_outer_position(PhysicalPosition::new(x, y));
        window.set_inner_size(PhysicalSize::new(w as u32, h as u32));
    }

    // 独立设置置顶，因为这个通常不需要和几何变换原子化
    window.set_always_on_top(always_on_top);
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

            window_init.set_focus();
        }
    });

    // 🔥🔥🔥 核心修复：移除所有 set_visible hack，优化顺序 🔥🔥🔥
    let window_effect = window.clone();
    use_effect(move || {
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

        // 获取当前窗口状态，用于判断是"变大"还是"变小"
        let current_width = window_effect.inner_size().width;
        let is_shrinking = current_width > 200 && mode == WindowMode::Widget;

        // 计算目标参数
        let (target_w_phys, target_h_phys, target_x_phys, target_y_phys, always_on_top) = match mode
        {
            WindowMode::Widget => {
                let (tx, ty) = if let Some(pos) = last_widget_pos() {
                    (pos.x, pos.y)
                } else {
                    let center_y = work_top + (work_h - CAPSULE_H) / 2.0;
                    let default_x = (work_w_phys as f64 / scale) - CAPSULE_W;
                    (
                        (default_x * scale).round() as i32,
                        (center_y * scale).round() as i32,
                    )
                };
                (
                    (CAPSULE_W * scale).round() as i32,
                    (CAPSULE_H * scale).round() as i32,
                    tx,
                    ty,
                    true,
                )
            }
            WindowMode::Main => {
                // 记录位置逻辑
                if window_effect.inner_size().width < 200 {
                    if let Ok(current_pos) = window_effect.outer_position() {
                        last_widget_pos.set(Some(current_pos));
                    }
                }

                let anchor_pos = last_widget_pos().unwrap_or(PhysicalPosition::new(0, 0));
                let anchor_x = anchor_pos.x as f64 / scale;
                let th = work_h - (MARGIN * 2.0);
                let ty = work_top + MARGIN;
                let screen_center_x = (work_x_phys as f64 / scale) + (work_w / 2.0);
                let tx = if anchor_x > screen_center_x {
                    (work_w_phys as f64 / scale) - CARD_W - MARGIN
                } else {
                    (work_x_phys as f64 / scale) + MARGIN
                };

                (
                    (CARD_W * scale).round() as i32,
                    (th * scale).round() as i32,
                    (tx * scale).round() as i32,
                    (ty * scale).round() as i32,
                    true,
                )
            }
            WindowMode::Settings => {
                let cx = (work_x_phys as f64 / scale) + (work_w - SETTINGS_W) / 2.0;
                let cy = work_top + (work_h - SETTINGS_H) / 2.0;
                (
                    (SETTINGS_W * scale).round() as i32,
                    (SETTINGS_H * scale).round() as i32,
                    (cx * scale).round() as i32,
                    (cy * scale).round() as i32,
                    false,
                )
            }
        };

        // 🔥🔥🔥 核心动画策略 🔥🔥🔥
        if is_shrinking {
            // === 场景：从大变小 (Settings/Main -> Widget) ===
            // 解决 "右侧瞬间渲染" 问题
            // 策略：1. 先原地变小 (视觉上：界面收缩)
            //       2. 再移动到角落 (视觉上：小球飞走)

            let win = window_effect.clone();
            spawn(async move {
                // 1. 获取当前中心点（为了原地收缩）
                if let Ok(curr_pos) = win.outer_position() {
                    let curr_size = win.inner_size();
                    // 计算出能保持中心点不变的新左上角坐标
                    // 新X = 旧X + (旧宽 - 新宽)/2
                    let center_fix_x = curr_pos.x + ((curr_size.width as i32 - target_w_phys) / 2);
                    let center_fix_y = curr_pos.y + ((curr_size.height as i32 - target_h_phys) / 2);

                    // 步骤 A: 原地变形 (保持 UI 在用户注视的位置)
                    atomic_update_window(
                        &win,
                        center_fix_x,
                        center_fix_y,
                        target_w_phys,
                        target_h_phys,
                        always_on_top,
                    );
                }

                // 2. 稍微停顿，让用户看清"它变小了"，并等待 Dioxus 渲染完小界面
                // 150ms 足够让 WebView 重绘，且不会觉得太慢
                tokio::time::sleep(Duration::from_millis(150)).await;

                // 步骤 B: 归位 (移动到右下角/锚点)
                atomic_update_window(
                    &win,
                    target_x_phys,
                    target_y_phys,
                    target_w_phys,
                    target_h_phys,
                    always_on_top,
                );
                win.set_focus();
            });
        } else {
            // === 场景：从小变大 (Widget -> Settings/Main) ===
            // 或者是 大变大 (Main <-> Settings)
            // 直接一步到位，因为"展开"通常不需要太复杂的过渡，瞬移到中心展开感觉是自然的
            atomic_update_window(
                &window_effect,
                target_x_phys,
                target_y_phys,
                target_w_phys,
                target_h_phys,
                always_on_top,
            );
            window_effect.set_focus();
        }
    });

    // 托盘点击逻辑
    use_future(move || {
        let window = window.clone();
        async move {
            let receiver = TrayIconEvent::receiver();
            loop {
                if let Ok(event) = receiver.try_recv() {
                    if let TrayIconEvent::Click { .. } = event {
                        window.set_visible(true);
                        window.set_focus();
                        window_mode.set(WindowMode::Main);
                    }
                }
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
        }
    });

    let mut messages =
        use_signal(|| vec![ChatMessage::new(0, "👋 嗨！把 Excel 拖进来开始吧。", false)]);
    let config = use_signal(|| load_config());
    let mut last_file_path = use_signal(|| String::new());
    let mut is_dragging = use_signal(|| false);
    let is_loading = use_signal(|| false);

    // 错误修复信号
    let mut error_fix_signal = use_signal(|| None::<String>);
    let mut retry_count = use_signal(|| 0);
    const MAX_RETRIES: i32 = 3;

    // 🔥 1. Confirm 回调
    let mut on_confirm = move |msg_id: usize| {
        // 🔥 修复 E0503: 获取值后立即释放锁，不要持有 MutexGuard 跨 await
        let pending_code_opt = {
            let mut msgs = messages.write();
            let msg = &mut msgs[msg_id];
            if let Some(code) = msg.pending_code.clone() {
                msg.status = ActionStatus::Running;
                // 备份文件
                let target_file = last_file_path();
                if !target_file.is_empty() {
                    // 🔥 修复 E0425: backup_file 已引入
                    msg.backup_path = backup_file(&target_file);
                }
                Some(code)
            } else {
                None
            }
        };

        if let Some(code) = pending_code_opt {
            spawn(async move {
                let res: anyhow::Result<String, String> = run_python_code(&code).await;

                let mut msgs = messages.write();
                if let Some(msg) = msgs.get_mut(msg_id) {
                    match res {
                        Ok(out) => {
                            msg.status = ActionStatus::Success;
                            msg.text.push_str(&format!("\n\n✨ 结果:\n{}", out));
                            // 成功后，重置重试计数器
                            retry_count.set(0);
                        }
                        Err(e) => {
                            msg.status = ActionStatus::Error(e.clone());
                            msg.text.push_str(&format!("\n\n❌ 错误:\n{}", e));
                            // 触发修复
                            let current_retries = *retry_count.read();
                            if current_retries < MAX_RETRIES {
                                // 没超过上限，继续自动修复
                                retry_count += 1;
                                msg.text.push_str(&format!(
                                    "\n\n🔄 自动修复中 (尝试 {}/{})...",
                                    current_retries + 1,
                                    MAX_RETRIES
                                ));
                                error_fix_signal.set(Some(e));
                            } else {
                                // 超过上限，放弃治疗
                                msg.text.push_str(&format!("\n\n🛑 已达到最大重试次数 ({})，停止自动修复。请检查提示词或手动修改代码。", MAX_RETRIES));
                                // 重置计数器，等待用户下次手动操作
                                retry_count.set(0);
                                // 注意：这里不再设置 error_fix_signal，循环中止
                            }
                        }
                    }
                }
            });
        }
    };

    // 手动点击执行时，也要重置计数器 (算作一次全新操作)
    let on_manual_confirm = move |id| {
        retry_count.set(0); // 用户手动点击了，说明是一次新的尝试，计数归零
        on_confirm(id);
    };

    // 🔥 2. Auto Run 回调 (逻辑完全一样，复制一份以避开 borrow checker)
    let on_auto_run = move |id| {
        on_confirm(id);
    };

    let on_cancel = move |id: usize| {
        let mut msgs = messages.write();
        if let Some(msg) = msgs.get_mut(id) {
            msg.status = ActionStatus::Cancelled;
            msg.pending_code = None;
            retry_count.set(0); // 取消也重置计数
        }
    };

    let on_undo = move |id: usize| {
        let mut msgs = messages.write();
        if let Some(msg) = msgs.get_mut(id) {
            if let Some(bk) = &msg.backup_path {
                let target = last_file_path();
                // 🔥 修复 E0425: restore_file 已引入
                match restore_file(&target, bk) {
                    Ok(_) => {
                        msg.status = ActionStatus::Undone;
                        msg.text.push_str("\n\n↩️ 已撤销");
                    }
                    Err(e) => msg.text.push_str(&format!("\n❌ 撤销失败: {}", e)),
                }
            }
        }
    };

    // 🔥 1. 判断聊天状态
    // 假设初始只有 1 条欢迎消息，当 > 1 时说明用户发话了
    let has_started_chat = messages.read().len() > 1;
    let content_mode_class = if has_started_chat {
        "content-area chat-mode"
    } else {
        "content-area center-mode"
    };

    // 🔥 2. 获取文件名用于显示
    let current_file = last_file_path();
    let file_name = if !current_file.is_empty() {
        Path::new(&current_file)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string()
    } else {
        String::new()
    };

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
                    on_close: move |_| window_mode.set(WindowMode::Widget),
                }
            }
        } else {
            div {
                class: "window-frame main-panel",
                oncontextmenu: move |evt| evt.prevent_default(),

                div { class: "panel-header",
                    div { class: "title-text", "Excel AI Agent" }
                    div {
                        class: "icon-btn",
                        title: "设置",
                        onclick: move |_| window_mode.set(WindowMode::Settings),
                        "⚙️"
                    }
                    div {
                        style: "cursor: pointer; padding: 5px;",
                        onclick: move |_| window_mode.set(WindowMode::Widget),
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
                        if let Some(first_file) = files.first() {
                            let fname = first_file.name();
                            let dir = std::env::current_dir().unwrap_or_default();
                            let path = dir.join(&fname).to_string_lossy().to_string();
                            last_file_path.set(path);
                        }
                    },

                    // 🔥 3. 应用动态布局 Class
                    div { class: "{content_mode_class}",

                        if is_dragging() {
                            div { class: "drag-overlay", "📂 投喂 Excel！" }
                        }

                        // 🔥 4. 文件悬浮胶囊
                        if !current_file.is_empty() {
                            div { class: "file-pill-container",
                                div { class: "file-pill",
                                    span { "📊 {file_name}" }
                                    span {
                                        class: "close-file",
                                        onclick: move |_| last_file_path.set(String::new()),
                                        title: "移除文件",
                                        "✕"
                                    }
                                }
                            }
                        }

                        // 聊天列表 (只有开始聊天后才显示)
                        if has_started_chat {
                            ChatView {
                                messages,
                                last_file_path,
                                on_confirm,
                                on_cancel,
                                on_undo,
                            }
                        } else {
                            // 🔥 5. 居中模式下的欢迎语 (代替之前的 ChatView)
                            div { style: "text-align: center; margin-bottom: 30px; color: #666; animation: fadeIn 0.5s;",
                                div { style: "font-size: 28px; font-weight: 900; color: #000; margin-bottom: 12px;",
                                    "Excel AI Agent"
                                }
                                div { "拖入表格，开始分析" }
                            }
                        }

                        // 输入区 (始终存在，位置由父容器 class 控制)
                        InputArea {
                            messages,
                            last_file_path,
                            is_loading,
                            config,
                            error_fix_signal,
                            on_run_code: on_auto_run,
                        }
                    }
                }
            }
        }
    }
}
