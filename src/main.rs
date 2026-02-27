#![allow(non_snake_case)]

mod components;
mod models;
mod services;

use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use dioxus::desktop::tao::platform::windows::WindowBuilderExtWindows;
use dioxus::desktop::trayicon::{Icon, MouseButton, TrayIconBuilder, TrayIconEvent};
use dioxus::desktop::wry::dpi::PhysicalPosition;
use dioxus::desktop::{
    use_tray_icon_event_handler, Config, LogicalPosition, LogicalSize, WindowBuilder,
};
use dioxus::prelude::*;
use futures_util::StreamExt;

use crate::components::dock_capsule::DockCapsule;
use crate::components::import_modals::ImportModal;
use crate::models::{ActionStatus, PendingImport, WindowMode};
use crate::services::config::load_config;
use crate::services::excel_engine::{ExcelEngine, FileSchema};
use crate::services::python::{create_batch_backups, run_batch_hot_undo, run_python_code};
use components::{chat_view::ChatView, input_area::InputArea, settings::Settings};
use models::ChatMessage;

#[cfg(target_os = "windows")]
use windows_sys::Win32::Foundation::RECT;
#[cfg(target_os = "windows")]
use windows_sys::Win32::UI::WindowsAndMessaging::{SystemParametersInfoW, SPI_GETWORKAREA};

fn main() {
    dioxus_logger::init(tracing::Level::INFO).expect("failed to init logger");

    // 初始化与清理
    services::python::init_python_env();
    services::python::cleanup_backups();

    // 崩溃钩子
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        println!("💥 程序发生严重错误，正在紧急清理临时文件...");
        services::python::cleanup_backups();
        // 继续执行默认的报错打印
        default_hook(info);
    }));

    let icon_bytes = include_bytes!("../assets/icon.png");
    let icon = load_icon(icon_bytes);

    let _tray = match icon {
        Ok(i) => Some(Box::leak(Box::new(
            TrayIconBuilder::new()
                .with_tooltip("Excel AI Agent\n左键：打开 | 右键：退出")
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
        .with_visible(true)
        .with_undecorated_shadow(false)
        .with_skip_taskbar(true)
        .with_always_on_top(true);

    let config = Config::new().with_window(window_builder);
    LaunchBuilder::desktop().with_cfg(config).launch(App);

    // 退出清理
    println!("🛑 程序正常退出，正在清理临时文件...");
    services::python::cleanup_backups();
}

fn load_icon(icon_bytes: &[u8]) -> anyhow::Result<Icon> {
    // 使用 load_from_memory 而不是 open
    let (icon_rgba, icon_width, icon_height) = {
        let image = image::load_from_memory(icon_bytes)?.into_rgba8();
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

    // 使用 use_tray_icon_event_handler 监听事件
    let window_tray = window.clone();
    use_tray_icon_event_handler(move |event: &TrayIconEvent| {
        match event {
            // 左键单击：打开/激活窗口
            TrayIconEvent::Click {
                button: MouseButton::Left,
                ..
            }
            | TrayIconEvent::DoubleClick {
                button: MouseButton::Left,
                ..
            } => {
                println!("✅ 托盘左键点击：激活窗口");
                window_tray.set_visible(true);
                window_tray.set_focus();
                window_mode.set(WindowMode::Main);
            }
            // 右键单击：退出程序
            TrayIconEvent::Click {
                button: MouseButton::Right,
                ..
            } => {
                println!("🛑 托盘右键点击：退出程序");
                std::process::exit(0);
            }
            _ => {}
        }
    });

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

    // 切换模式并保存当前位置
    let window_change_mode = window.clone();
    let mut change_mode = move |target_mode: WindowMode| {
        // 如果当前是 Widget 模式，说明用户可能拖动过，立刻保存当前真实坐标
        if window_mode() == WindowMode::Widget {
            if let Ok(current_pos) = window_change_mode.outer_position() {
                last_widget_pos.set(Some(current_pos));
            }
        }
        // 然后再切换模式，触发 Effect
        window_mode.set(target_mode);
    };

    // 3. 窗口响应 Effect (只响应 mode 变化)
    let window_effect = window.clone();
    use_effect(move || {
        let mode = window_mode(); // 订阅模式变化
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

    // --- 状态定义 ---
    let mut messages =
        use_signal(|| vec![ChatMessage::new(0, "👋 嗨！把 Excel 拖进来开始吧。", false)]);
    let config = use_signal(|| load_config());
    // 多文件状态
    let mut active_files = use_signal(|| Vec::<String>::new());
    let is_loading = use_signal(|| false);
    // 错误修复信号
    let mut error_fix_signal = use_signal(|| None::<String>);
    let mut retry_count = use_signal(|| 0);

    let mut pending_import = use_signal(|| None::<PendingImport>);
    let mut global_schemas = use_signal(|| HashMap::<String, FileSchema>::new());
    const MAX_RETRIES: i32 = 3;

    // 文件处理通道
    let tx_files = use_coroutine(move |mut rx: UnboundedReceiver<String>| async move {
        // 🔥 修复：现在 rx.next() 可以工作了，因为引入了 StreamExt
        while let Some(path) = rx.next().await {
            println!("👉 Coroutine 收到文件: {}", path); // 打印日志
            let mut current = active_files.write();
            if !current.contains(&path) {
                let new_id = messages.read().len();
                let file_name = Path::new(&path)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy();
                messages.write().push(ChatMessage::new(
                    new_id,
                    &format!("📄 收到文件: {}", file_name),
                    false,
                ));
                current.push(path);
                window_mode.set(WindowMode::Main);
            }
        }
    });

    // 打开文件对话框的函数
    let open_file_dialog = move |_| {
        spawn(async move {
            // 使用 rfd 弹出原生选择框
            if let Some(path) = rfd::AsyncFileDialog::new()
                .add_filter("Excel", &["xlsx", "xls", "xlsm"])
                .pick_file()
                .await
            {
                let full_path = path.path().to_string_lossy().to_string();
                tx_files.send(full_path.clone());

                match ExcelEngine::get_sheet_names(&full_path) {
                    Ok(sheet_names) => {
                        pending_import.set(Some(PendingImport::new(full_path, sheet_names)));
                    }
                    Err(e) => {
                        println!("读取Excel失败: {}", e);
                    }
                }
            }
        });
    };

    // 弹窗确认逻辑：用户配好表头行数，点击“确认导入”
    let mut handle_import_confirm =
        move |(file_path, sheet_configs): (String, HashMap<String, usize>)| {
            spawn(async move {
                // 调用 Rust 引擎进行精准重度解析
                match ExcelEngine::parse_file_with_config(&file_path, &sheet_configs) {
                    Ok(schema) => {
                        // 1. 存入全局记忆库
                        global_schemas.write().insert(file_path.clone(), schema);

                        // 2. 将文件加入活跃列表
                        let mut files = active_files.write();
                        if !files.contains(&file_path) {
                            files.push(file_path);
                        }

                        // 3. 关闭弹窗
                        pending_import.set(None);
                    }
                    Err(e) => {
                        println!("精准解析失败: {}", e);
                        pending_import.set(None);
                    }
                }
            });
        };

    // 弹窗取消逻辑
    let mut handle_import_cancel = move || {
        pending_import.set(None); // 直接关闭弹窗
    };

    // 🔥 1. Confirm 回调
    let mut on_confirm = move |msg_id: usize| {
        // 1. 获取指令，但不在这里备份（因为 backup_file 现在是 async 的）
        let (code_opt, current_files, has_existing_backup) = {
            let mut msgs = messages.write();
            let msg = &mut msgs[msg_id];
            let code = msg.pending_code.clone();
            if code.is_some() {
                msg.status = ActionStatus::Running;
            }
            // 检查当前消息是否已经关联了备份文件
            let has_backup = msg.backup_paths.is_some();

            // 返回需要的数据供 async 块使用
            (code, active_files.read().clone(), has_backup)
        };

        if let Some(code) = code_opt {
            spawn(async move {
                // 1. 批量备份当前所有活跃文件
                // 只有成功备份的文件，之后才会被记录到 Undo 列表里
                // 这样可以防止自动修复过程中的“脏文件”覆盖了“原始文件”的备份
                if !has_existing_backup {
                    let backups = if !current_files.is_empty() {
                        create_batch_backups(current_files).await
                    } else {
                        Vec::new()
                    };

                    // 2. 记录备份路径到消息中
                    if !backups.is_empty() {
                        messages.write()[msg_id].backup_paths = Some(backups);
                    }
                } else {
                    println!("🛡️ 检测到已有备份，跳过本次备份，保留原始快照。");
                }

                // 4. 执行 AI 代码
                let res = run_python_code(&code).await;
                // 结果处理
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

    // 级联回溯批量撤销逻辑
    let on_undo = move |target_msg_id: usize| {
        let backup_pairs = {
            let msgs = messages.read();
            msgs.get(target_msg_id).and_then(|m| m.backup_paths.clone())
        };

        if let Some(pairs) = backup_pairs {
            spawn(async move {
                // 尝试批量热恢复
                let res = run_batch_hot_undo(pairs).await;

                let mut msgs = messages.write();
                let len = msgs.len();

                // 级联失效处理
                for i in target_msg_id..len {
                    if let Some(m) = msgs.get_mut(i) {
                        if matches!(m.status, ActionStatus::Success | ActionStatus::Running) {
                            m.status = ActionStatus::Undone;
                            if i == target_msg_id {
                                match res {
                                    Ok(ref log) => m.text.push_str(&format!("\n\n{}", log)),
                                    Err(ref e) => m.text.push_str(&format!("\n❌ 撤销出错: {}", e)),
                                }
                            } else {
                                m.text.push_str("\n(因回溯已失效)");
                            }
                        }
                    }
                }
            });
        }
    };

    let mut remove_file = move |path: String| {
        let mut files = active_files.write();
        files.retain(|f| f != &path);
    };

    // 清空所有文件
    let clear_all_files = move |_| {
        active_files.write().clear();
    };

    // 🔥 1. 判断聊天状态
    // 假设初始只有 1 条欢迎消息，当 > 1 时说明用户发话了
    let has_started_chat = messages.read().len() > 1;
    let content_mode_class = if has_started_chat {
        "content-area chat-mode"
    } else {
        "content-area center-mode"
    };

    let file_list_data = active_files.read().clone();
    let file_count = file_list_data.len();
    let file_list_elements = file_list_data.iter().map(|file_path| {
        let p = file_path.clone();
        let name = Path::new(&p)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        // 根据扩展名给一点不同的视觉
        let ext = Path::new(&p)
            .extension()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string()
            .to_uppercase();

        rsx! {
            div { class: "file-card", title: "{p}", // hover 显示全路径
                div { class: "file-icon-box",
                    div { class: "file-icon-text", "{ext}" } // 显示 XLSX / CSV
                }
                div { class: "file-info",
                    div { class: "file-name", "{name}" }
                }
                div {
                    class: "file-remove-btn",
                    onclick: move |evt| {
                        evt.stop_propagation();
                        remove_file(p.clone());
                    },
                    "✕"
                }
            }
        }
    });

    rsx! {
        document::Stylesheet { href: asset!("/assets/lib/atom-one-dark.min.css") }
        document::Stylesheet { href: asset!("/assets/main.css") }
        script { src: asset!("/assets/lib/highlight.min.js") }
        script { src: asset!("/assets/lib/python.min.js") }

        if window_mode() == WindowMode::Widget {
            DockCapsule {
                window_mode,
                messages,
                last_file_path: use_signal(|| active_files.read().first().cloned().unwrap_or_default()),
                on_switch_mode: change_mode, // 传入回调
            }
        } else if window_mode() == WindowMode::Settings {
            div {
                class: "window-frame settings-panel",
                oncontextmenu: move |evt| evt.prevent_default(),
                Settings {
                    config,
                    on_close: move |_| change_mode(WindowMode::Widget),
                }
            }
        } else {
            div {
                class: "window-frame main-panel",
                oncontextmenu: move |evt| evt.prevent_default(),

                div { class: "panel-header",
                    div { class: "title-text", "Excel AI Agent" }
                    div {
                        style: "cursor: pointer; padding: 5px;",
                        onclick: move |_| change_mode(WindowMode::Widget),
                        "⏬"
                    }
                }

                div { class: "app-container",
                    // 3. 应用动态布局 Class
                    div { class: "{content_mode_class}",
                        if !active_files.read().is_empty() {
                            div { class: "workspace-panel",
                                div { class: "workspace-header",
                                    div { class: "workspace-title", "📂 工作区 ({file_count})" }
                                    div {
                                        class: "workspace-clear-btn",
                                        onclick: clear_all_files,
                                        "清空全部"
                                    }
                                }
                                div { class: "file-card-scroll", {file_list_elements} }
                            }
                        }

                        // 聊天列表 (只有开始聊天后才显示)
                        if has_started_chat {
                            ChatView {
                                messages,
                                last_file_path: use_signal(|| String::new()), // 兼容参数
                                on_confirm: on_manual_confirm,
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
                            active_files,
                            is_loading,
                            config,
                            error_fix_signal,
                            on_run_code: on_auto_run,
                            on_open_file: open_file_dialog,
                        }
                    }
                } // <--- app-container 结束
                if pending_import.read().is_some() {
                    ImportModal {
                        pending_import,
                        on_confirm: handle_import_confirm,
                        on_cancel: handle_import_cancel,
                    }
                }
            } // <--- main-panel 结束
        }
    }
}
