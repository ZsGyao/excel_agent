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
use crate::services::python::{create_live_backup, restore_file, run_hot_undo, run_python_code};
use components::{chat_view::ChatView, input_area::InputArea, settings::Settings};
use models::ChatMessage;

#[cfg(target_os = "windows")]
use windows_sys::Win32::Foundation::RECT;
#[cfg(target_os = "windows")]
use windows_sys::Win32::UI::WindowsAndMessaging::{SystemParametersInfoW, SPI_GETWORKAREA};

fn main() {
    dioxus_logger::init(tracing::Level::INFO).expect("failed to init logger");
    services::python::init_python_env();

    // 启动时清理（防止上次强杀残留）
    services::python::cleanup_backups();
    // 注册崩溃钩子（防止程序 Panic 时残留）
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        println!("💥 程序发生严重错误，正在紧急清理临时文件...");
        services::python::cleanup_backups();
        // 继续执行默认的报错打印
        default_hook(info);
    }));

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

    // 正常关闭时清理
    println!("🛑 程序正常退出，正在清理临时文件...");
    services::python::cleanup_backups();
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
        // 1. 获取指令，但不在这里备份（因为 backup_file 现在是 async 的）
        let (code_opt, target_file) = {
            let mut msgs = messages.write();
            let msg = &mut msgs[msg_id];
            let code = msg.pending_code.clone();
            if code.is_some() {
                msg.status = ActionStatus::Running;
            }
            (code, last_file_path())
        };

        if let Some(code) = code_opt {
            spawn(async move {
                // 2. 异步创建【热备份】 (SaveCopyAs)
                // 这会保存当前的内存状态，解决 "Undo 无效" 问题
                let backup_path = if !target_file.is_empty() {
                    match create_live_backup(&target_file).await {
                        Ok(p) => Some(p),
                        Err(e) => {
                            println!("⚠️ 备份失败: {}", e);
                            None
                        }
                    }
                } else {
                    None
                };

                // 3. 将备份路径写入消息
                if let Some(bp) = backup_path {
                    messages.write()[msg_id].backup_path = Some(bp);
                }

                // 4. 执行 AI 代码
                let res = run_python_code(&code).await;
                let mut msgs = messages.write();
                if let Some(msg) = msgs.get_mut(msg_id) {
                    match res {
                        Ok(out) => {
                            msg.status = ActionStatus::Success;
                            msg.text.push_str(&format!("\n\n✨ 结果:\n{}", out));
                            retry_count.set(0);
                        }
                        Err(e) => {
                            msg.status = ActionStatus::Error(e.clone());
                            msg.text.push_str(&format!("\n\n❌ 错误:\n{}", e));
                            let current_retries = *retry_count.read();
                            if current_retries < MAX_RETRIES {
                                retry_count += 1;
                                msg.text.push_str(&format!(
                                    "\n\n🔄 自动修复中 (尝试 {}/{})...",
                                    current_retries + 1,
                                    MAX_RETRIES
                                ));
                                error_fix_signal.set(Some(e));
                            } else {
                                msg.text.push_str(&format!(
                                    "\n\n🛑 已达到最大重试次数 ({})，停止自动修复。",
                                    MAX_RETRIES
                                ));
                                retry_count.set(0);
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

    // 级联回溯撤销
    let on_undo = move |target_msg_id: usize| {
        // 1. 获取必要信息 (避免在该 async块 中长时间持有锁)
        let (backup_path, target_file) = {
            let msgs = messages.read();
            if let Some(msg) = msgs.get(target_msg_id) {
                (msg.backup_path.clone(), last_file_path())
            } else {
                (None, String::new())
            }
        };

        if let Some(bk) = backup_path {
            spawn(async move {
                // 执行恢复逻辑 (优先物理恢复，失败则热恢复)
                let restore_result = match restore_file(&target_file, &bk) {
                    Ok(_) => Ok("物理恢复"),
                    Err(_) => {
                        // 物理失败，尝试热恢复
                        match run_hot_undo(&target_file, &bk).await {
                            Ok(_) => Ok("热撤销"),
                            Err(e) => Err(e),
                        }
                    }
                };

                // 更新 UI 状态：级联标记失效
                let mut msgs = messages.write();

                match restore_result {
                    Ok(method) => {
                        // 🔥 重点：从 target_id 开始，直到最后一条消息
                        // 将所有 "Success" 的消息都标记为 "Undone"，因为文件已经回滚到了它们的过去
                        let len = msgs.len();
                        for i in target_msg_id..len {
                            if let Some(msg) = msgs.get_mut(i) {
                                // 只有处于成功状态的才需要标记为“已撤销”
                                // 或者是正在运行的，也强制取消
                                if matches!(
                                    msg.status,
                                    ActionStatus::Success | ActionStatus::Running
                                ) {
                                    msg.status = ActionStatus::Undone;

                                    // 仅在触发撤销的那条消息上显示详细提示
                                    if i == target_msg_id {
                                        msg.text.push_str(&format!(
                                            "\n\n✨ 成功回溯 ({})！此操作及后续操作已撤销。",
                                            method
                                        ));
                                    } else {
                                        // 后续被波及的消息，只加一个简单标记
                                        msg.text.push_str("\n\n(因历史回溯，此操作已失效)");
                                    }
                                }
                            }
                        }

                        // 也可以选择在底部插入一条新系统消息告诉用户
                        // msgs.push(ChatMessage::new(msgs.len(), "🔄 时间线已重置到指定节点。", false));
                    }
                    Err(e) => {
                        if let Some(msg) = msgs.get_mut(target_msg_id) {
                            msg.text.push_str(&format!("\n❌ 回溯失败: {}", e));
                        }
                    }
                }
            });
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
