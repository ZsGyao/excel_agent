#![allow(non_snake_case)]

mod components;
mod controllers;
mod models;
mod services;
mod store;
mod utils;

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
use crate::models::WindowMode;

use crate::controllers::{chat_controller, file_controller};
use crate::store::app_state::use_init_app_state;
use crate::utils::window::{atomic_update_window, get_work_area_rect}; // 🌟 使用控制器

use components::{chat_view::ChatView, input_area::InputArea, settings::Settings};

fn main() {
    dioxus_logger::init(tracing::Level::DEBUG).expect("failed to init logger");
    services::python::init_python_env();
    services::python::cleanup_backups();

    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        error!("App is panic, clean temp save file...");
        services::python::cleanup_backups();
        default_hook(info);
    }));

    let icon = load_icon(include_bytes!("../assets/icon.png"));
    let _tray = icon.ok().map(|i| {
        Box::leak(Box::new(
            TrayIconBuilder::new()
                .with_tooltip("Excel AI Agent")
                .with_icon(i)
                .build()
                .unwrap(),
        ))
    });

    let window_builder = WindowBuilder::new()
        .with_title("Excel Agent")
        .with_inner_size(LogicalSize::new(130.0, 160.0))
        .with_decorations(false)
        .with_transparent(true)
        .with_visible(true)
        .with_undecorated_shadow(false)
        .with_skip_taskbar(true)
        .with_always_on_top(true);

    LaunchBuilder::desktop()
        .with_cfg(Config::new().with_window(window_builder))
        .launch(App);
    services::python::cleanup_backups();
}

fn load_icon(icon_bytes: &[u8]) -> anyhow::Result<Icon> {
    let image = image::load_from_memory(icon_bytes)?.into_rgba8();
    let (width, height) = image.dimensions();
    Ok(Icon::from_rgba(image.into_raw(), width, height)?)
}

#[component]
fn App() -> Element {
    let window = dioxus::desktop::use_window();
    let mut state = use_init_app_state();

    const CAPSULE_W: f64 = 130.0;
    const CAPSULE_H: f64 = 160.0;
    const CARD_W: f64 = 480.0;
    const SETTINGS_W: f64 = 750.0;
    const SETTINGS_H: f64 = 550.0;
    const MARGIN: f64 = 60.0;

    let window_tray = window.clone();
    use_tray_icon_event_handler(move |event: &TrayIconEvent| match event {
        TrayIconEvent::Click {
            button: MouseButton::Left,
            ..
        }
        | TrayIconEvent::DoubleClick {
            button: MouseButton::Left,
            ..
        } => {
            window_tray.set_visible(true);
            window_tray.set_focus();
            state.window_mode.set(WindowMode::Main);
        }
        TrayIconEvent::Click {
            button: MouseButton::Right,
            ..
        } => std::process::exit(0),
        _ => {}
    });

    let window_init = window.clone();
    use_effect(move || {
        if let Some(monitor) = window_init.current_monitor() {
            let scale = monitor.scale_factor();
            let (work_w_phys, work_h_phys, _, work_y_phys) = get_work_area_rect();
            let center_y = (work_y_phys as f64 / scale) + (work_h_phys as f64 / scale / 2.0) - 25.0;
            let default_x = (work_w_phys as f64 / scale) - CAPSULE_W;
            window_init.set_outer_position(LogicalPosition::new(default_x, center_y));
            state.last_widget_pos.set(Some(PhysicalPosition::new(
                (default_x * scale).round() as i32,
                (center_y * scale).round() as i32,
            )));
            window_init.set_focus();
        }
    });

    let window_change_mode = window.clone();
    let mut change_mode = move |target_mode: WindowMode| {
        if (state.window_mode)() == WindowMode::Widget {
            if let Ok(pos) = window_change_mode.outer_position() {
                state.last_widget_pos.set(Some(pos));
            }
        }
        state.window_mode.set(target_mode);
    };

    let window_effect = window.clone();
    use_effect(move || {
        let mode = (state.window_mode)();
        if let Some(monitor) = window_effect.current_monitor() {
            let scale = monitor.scale_factor();
            let (work_w_phys, work_h_phys, work_x_phys, work_y_phys) = get_work_area_rect();
            let work_w = work_w_phys as f64 / scale;
            let work_h = work_h_phys as f64 / scale;
            let work_top = work_y_phys as f64 / scale;

            let is_shrinking = window_effect.inner_size().width > 200 && mode == WindowMode::Widget;

            let (target_w_phys, target_h_phys, target_x_phys, target_y_phys, always_on_top) =
                match mode {
                    WindowMode::Widget => {
                        let (tx, ty) = (state.last_widget_pos)()
                            .map(|p| (p.x, p.y))
                            .unwrap_or_else(|| {
                                (
                                    (((work_w_phys as f64 / scale) - CAPSULE_W) * scale).round()
                                        as i32,
                                    ((work_top + (work_h - CAPSULE_H) / 2.0) * scale).round()
                                        as i32,
                                )
                            });
                        (
                            (CAPSULE_W * scale).round() as i32,
                            (CAPSULE_H * scale).round() as i32,
                            tx,
                            ty,
                            true,
                        )
                    }
                    WindowMode::Main => {
                        if window_effect.inner_size().width < 200 {
                            if let Ok(pos) = window_effect.outer_position() {
                                state.last_widget_pos.set(Some(pos));
                            }
                        }
                        let anchor_x = (state.last_widget_pos)()
                            .unwrap_or(PhysicalPosition::new(0, 0))
                            .x as f64
                            / scale;
                        let tx = if anchor_x > (work_x_phys as f64 / scale) + (work_w / 2.0) {
                            (work_w_phys as f64 / scale) - CARD_W - MARGIN
                        } else {
                            (work_x_phys as f64 / scale) + MARGIN
                        };
                        (
                            (CARD_W * scale).round() as i32,
                            ((work_h - (MARGIN * 2.0)) * scale).round() as i32,
                            (tx * scale).round() as i32,
                            ((work_top + MARGIN) * scale).round() as i32,
                            true,
                        )
                    }
                    WindowMode::Settings => (
                        (SETTINGS_W * scale).round() as i32,
                        (SETTINGS_H * scale).round() as i32,
                        (((work_x_phys as f64 / scale) + (work_w - SETTINGS_W) / 2.0) * scale)
                            .round() as i32,
                        ((work_top + (work_h - SETTINGS_H) / 2.0) * scale).round() as i32,
                        false,
                    ),
                };

            if is_shrinking {
                let win = window_effect.clone();
                spawn(async move {
                    if let Ok(curr_pos) = win.outer_position() {
                        let curr_size = win.inner_size();
                        atomic_update_window(
                            &win,
                            curr_pos.x + ((curr_size.width as i32 - target_w_phys) / 2),
                            curr_pos.y + ((curr_size.height as i32 - target_h_phys) / 2),
                            target_w_phys,
                            target_h_phys,
                            always_on_top,
                        );
                    }
                    tokio::time::sleep(Duration::from_millis(150)).await;
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
        }
    });

    // 🌟 后台协程
    let tx_files = use_coroutine(move |mut rx: UnboundedReceiver<String>| async move {
        while let Some(path) = rx.next().await {
            let mut current = state.active_files.write();
            if !current.contains(&path) {
                let new_id = state.messages.read().len();
                let file_name = Path::new(&path)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy();
                state.messages.write().push(crate::models::ChatMessage::new(
                    new_id,
                    &format!("📄 收到文件: {}", file_name),
                    false,
                ));
                current.push(path);
                state.window_mode.set(WindowMode::Main);
            }
        }
    });

    let has_started_chat = state.messages.read().len() > 1;
    let file_list_data = state.active_files.read().clone();
    let file_count = file_list_data.len();
    let file_list_elements = file_list_data.iter().map(|file_path| {
        let p = file_path.clone();
        let name = Path::new(&p)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let ext = Path::new(&p)
            .extension()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string()
            .to_uppercase();
        rsx! {
            div { class: "file-card", title: "{p}",
                div { class: "file-icon-box",
                    div { class: "file-icon-text", "{ext}" }
                }
                div { class: "file-info",
                    div { class: "file-name", "{name}" }
                }
                div {
                    class: "file-remove-btn",
                    onclick: move |evt| {
                        evt.stop_propagation();
                        file_controller::remove_file(state, p.clone());
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

        if (state.window_mode)() == WindowMode::Widget {
            DockCapsule {
                window_mode: state.window_mode,
                messages: state.messages,
                last_file_path: use_signal(|| state.active_files.read().first().cloned().unwrap_or_default()),
                on_switch_mode: change_mode,
            }
        } else if (state.window_mode)() == WindowMode::Settings {
            div {
                class: "window-frame settings-panel",
                oncontextmenu: move |evt| evt.prevent_default(),
                Settings {
                    config: state.config,
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
                    div { class: if has_started_chat { "content-area chat-mode" } else { "content-area center-mode" },
                        if !state.active_files.read().is_empty() {
                            div { class: "workspace-panel",
                                div { class: "workspace-header",
                                    div { class: "workspace-title", "📂 工作区 ({file_count})" }
                                    div {
                                        class: "workspace-clear-btn",
                                        onclick: move |_| file_controller::clear_all_files(state),
                                        "清空全部"
                                    }
                                }
                                div { class: "file-card-scroll", {file_list_elements} }
                            }
                        }

                        if has_started_chat {
                            ChatView {
                                messages: state.messages,
                                last_file_path: use_signal(|| String::new()),
                                on_confirm: move |id| {
                                    state.retry_count.set(0);
                                    chat_controller::on_confirm(state, id);
                                },
                                on_cancel: move |id| chat_controller::on_cancel(state, id),
                                on_undo: move |id| chat_controller::on_undo(state, id),
                            }
                        } else {
                            div { style: "text-align: center; margin-bottom: 30px; color: #666; animation: fadeIn 0.5s;",
                                div { style: "font-size: 28px; font-weight: 900; color: #000; margin-bottom: 12px;",
                                    "Excel AI Agent"
                                }
                                div { "拖入表格，开始分析" }
                            }
                        }

                        InputArea {
                            messages: state.messages,
                            active_files: state.active_files,
                            is_loading: state.is_loading,
                            config: state.config,
                            on_run_code: move |id| chat_controller::on_confirm(state, id),
                            on_open_file: move |_| file_controller::open_file_dialog(state, tx_files.clone()),
                        }
                    }
                }
                if state.pending_import.read().is_some() {
                    ImportModal {
                        pending_import: state.pending_import,
                        on_confirm: move |(path, config)| file_controller::handle_import_confirm(state, path, config),
                        on_cancel: move |_| file_controller::handle_import_cancel(state),
                    }
                }
            }
        }
    }
}
