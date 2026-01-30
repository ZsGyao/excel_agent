use crate::models::{ChatMessage, WindowMode};
use dioxus::{
    core::{Element, Event, Task},
    desktop::{
        use_window,
        wry::dpi::{LogicalPosition, LogicalSize, PhysicalPosition},
        DesktopContext,
    },
    hooks::use_signal,
    html::{InteractionLocation, MouseData},
    prelude::*,
};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DockSide {
    Left,
    Right,
}

#[cfg(target_os = "windows")]
fn get_hwnd(window: &DesktopContext) -> Option<windows_sys::Win32::Foundation::HWND> {
    use windows_sys::Win32::Foundation::HWND;
    if let Ok(handle) = window.window_handle() {
        if let RawWindowHandle::Win32(win32_handle) = handle.as_raw() {
            return Some(win32_handle.hwnd.get() as HWND);
        }
    }
    None
}

fn atomic_update_bounds(window: &DesktopContext, x: f64, y: f64, w: f64, h: f64) {
    let scale = window
        .current_monitor()
        .map(|m| m.scale_factor())
        .unwrap_or(1.0);
    let phys_x = (x * scale).round() as i32;
    let phys_y = (y * scale).round() as i32;
    let phys_w = (w * scale).round() as i32;
    let phys_h = (h * scale).round() as i32;

    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            SetWindowPos, SWP_NOACTIVATE, SWP_NOZORDER,
        };
        if let Some(hwnd) = get_hwnd(window) {
            unsafe {
                SetWindowPos(
                    hwnd,
                    std::ptr::null_mut(),
                    phys_x,
                    phys_y,
                    phys_w,
                    phys_h,
                    SWP_NOACTIVATE | SWP_NOZORDER,
                );
            }
            return;
        }
    }
    window.set_outer_position(LogicalPosition::new(x, y));
    window.set_inner_size(LogicalSize::new(w, h));
}

#[component]
pub fn DockCapsule(
    mut window_mode: Signal<WindowMode>,
    mut messages: Signal<Vec<ChatMessage>>,
    mut last_file_path: Signal<String>,
) -> Element {
    let window = use_window();
    let mut dock_side = use_signal(|| DockSide::Right);
    let mut is_pinned = use_signal(|| false);
    let mut is_hovering = use_signal(|| false);
    let mut drag_start_offset = use_signal(|| (0.0, 0.0));
    let mut is_dragging = use_signal(|| false);
    let mut debounce_task = use_signal(|| None::<Task>);
    let mut anim_ready = use_signal(|| false);

    // 🔥 必须与 main.rs 一致
    const EXPANDED_W: f64 = 130.0;
    const EXPANDED_H: f64 = 160.0;

    // 初始化位置检测
    let window_init = window.clone();
    use_effect(move || {
        if let Some(monitor) = window_init.current_monitor() {
            let scale = monitor.scale_factor();
            let screen_w = monitor.size().width as f64 / scale;
            if let Ok(pos) = window_init.outer_position() {
                let x = pos.x as f64 / scale;
                if x < screen_w / 2.0 {
                    dock_side.set(DockSide::Left);
                } else {
                    dock_side.set(DockSide::Right);
                }
            }
        }
        spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            anim_ready.set(true);
        });
    });

    // 拖拽逻辑
    let window_drag_loop = window.clone();
    use_effect(move || {
        if is_dragging() {
            let window_async = window_drag_loop.clone();
            spawn(async move {
                loop {
                    #[cfg(target_os = "windows")]
                    {
                        use windows_sys::Win32::Foundation::POINT;
                        use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
                            GetAsyncKeyState, VK_LBUTTON,
                        };
                        use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;
                        unsafe {
                            if (GetAsyncKeyState(VK_LBUTTON as i32) as u16 & 0x8000) == 0 {
                                is_dragging.set(false);
                                break;
                            }
                            let mut point = POINT { x: 0, y: 0 };
                            GetCursorPos(&mut point);
                            if let Some(monitor) = window_async.current_monitor() {
                                let scale = monitor.scale_factor();
                                let offset = drag_start_offset();
                                let mouse_x = point.x as f64 / scale;
                                let mouse_y = point.y as f64 / scale;
                                atomic_update_bounds(
                                    &window_async,
                                    mouse_x - offset.0,
                                    mouse_y - offset.1,
                                    EXPANDED_W,
                                    EXPANDED_H,
                                );
                            }
                        }
                    }
                    tokio::time::sleep(Duration::from_millis(8)).await;
                }
                is_hovering.set(false);
                if let Some(monitor) = window_async.current_monitor() {
                    let scale = monitor.scale_factor();
                    let screen_w = monitor.size().width as f64 / scale;
                    let pos = window_async
                        .outer_position()
                        .unwrap_or(PhysicalPosition::new(0, 0));
                    let x = pos.x as f64 / scale;
                    let y = pos.y as f64 / scale;
                    if x < screen_w / 2.0 {
                        dock_side.set(DockSide::Left);
                        atomic_update_bounds(&window_async, 0.0, y, EXPANDED_W, EXPANDED_H);
                    } else {
                        dock_side.set(DockSide::Right);
                        atomic_update_bounds(
                            &window_async,
                            screen_w - EXPANDED_W,
                            y,
                            EXPANDED_W,
                            EXPANDED_H,
                        );
                    }
                }
            });
        }
    });

    let handle_mouse_down = move |evt: Event<MouseData>| {
        evt.prevent_default();
        evt.stop_propagation();
        if is_pinned() {
            return;
        }
        let coords = evt.client_coordinates();
        drag_start_offset.set((coords.x, coords.y));
        is_dragging.set(true);
    };
    let handle_enter = move |_| {
        if !anim_ready() || is_dragging() {
            return;
        }
        if let Some(task) = debounce_task.write().take() {
            task.cancel();
        }
        is_hovering.set(true);
    };
    let handle_leave = move |_| {
        if is_dragging() {
            return;
        }
        let task = spawn(async move {
            tokio::time::sleep(Duration::from_millis(200)).await;
            is_hovering.set(false);
        });
        debounce_task.set(Some(task));
    };

    let container_cls = format!(
        "dock-container {}",
        if dock_side() == DockSide::Left {
            "left"
        } else {
            "right"
        }
    );

    // 🔥 wrapper_cls 控制整体展开
    let wrapper_cls = format!(
        "dock-capsule {} {}",
        if is_hovering() { "expanded" } else { "" },
        if !anim_ready() { "no-anim" } else { "" }
    );

    rsx! {
        div {
            class: "{container_cls}",
            // 垂直布局对齐：左吸附靠左，右吸附靠右
            style: if dock_side() == DockSide::Right { "align-items: flex-end;" } else { "align-items: flex-start;" },

            // 🔥 核心结构变化：外层是 dock-capsule (transparent wrapper)
            div { class: "{wrapper_cls}", onmouseleave: handle_leave,

                // === 1. 上层：主胶囊 (图标 + 文字) ===
                div {
                    class: "main-capsule",
                    onmousedown: handle_mouse_down,
                    onmouseenter: handle_enter,

                    img {
                        class: "app-icon",
                        src: asset!("assets/icon.png"),
                        draggable: false,
                    }

                    // 文字按钮：点击去聊天
                    div {
                        class: "primary-action",
                        onclick: move |evt| {
                            evt.stop_propagation();
                            if let Some(task) = debounce_task.write().take() {
                                task.cancel();
                            }
                            window_mode.set(WindowMode::Main);
                        },
                        "💬 Chat"
                    }
                }

                // === 2. 下层：功能网格 (向下滑出) ===
                div { class: "secondary-grid",
                    // 设置
                    div {
                        class: "grid-btn settings",
                        title: "配置",
                        onclick: move |evt| {
                            evt.stop_propagation();
                            if let Some(task) = debounce_task.write().take() {
                                task.cancel();
                            }
                            window_mode.set(WindowMode::Settings);
                        },
                        "⚙️"
                    }
                    // 置顶
                    div {
                        class: if is_pinned() { "grid-btn pin active" } else { "grid-btn pin" },
                        title: "置顶",
                        onclick: move |evt| {
                            evt.stop_propagation();
                            is_pinned.set(!is_pinned());
                        },
                        "📌"
                    }
                    // 更多 (占位)
                    div { class: "grid-btn more", "…" }
                }
            }
        }
    }
}
