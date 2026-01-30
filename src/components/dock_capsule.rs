use std::time::Duration;

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

// 🔥 核心修正：直接使用 crate，不要用 dioxus::desktop::tao::...
// 这行代码能工作的前提是你 Cargo.toml 里加了 raw-window-handle = "0.6"
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

use crate::models::{ChatMessage, WindowMode};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DockSide {
    Left,
    Right,
}

// 原子更新函数：用于初始吸附
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
        use windows_sys::Win32::Foundation::HWND;
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            SetWindowPos, SWP_NOACTIVATE, SWP_NOZORDER,
        };

        // 1. 获取句柄 (来自 raw_window_handle crate)
        if let Ok(handle) = window.window_handle() {
            // 2. 匹配 Win32
            if let RawWindowHandle::Win32(win32_handle) = handle.as_raw() {
                // 3. 转换
                let hwnd_isize = win32_handle.hwnd.get();
                let hwnd = hwnd_isize as HWND;

                unsafe {
                    SetWindowPos(
                        hwnd,
                        std::ptr::null_mut(),
                        phys_x,
                        phys_y,
                        phys_w,
                        phys_h,
                        SWP_NOZORDER | SWP_NOACTIVATE,
                    );
                }
                return;
            }
        }
    }

    // 非 Windows 降级
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

    // 常量：物理窗口始终保持最大宽度，利用透明区域防闪烁
    const EXPANDED_W: f64 = 140.0;
    const EXPANDED_H: f64 = 56.0;

    // 1. 鼠标按下
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

    // 2. 鼠标移动 (拖拽)
    let window_move = window.clone();
    let handle_mouse_move = move |evt: Event<MouseData>| {
        if is_dragging() {
            let screen_coords = evt.screen_coordinates();
            let offset = drag_start_offset();
            // 拖拽时移动整个窗口
            window_move.set_outer_position(LogicalPosition::new(
                screen_coords.x - offset.0,
                screen_coords.y - offset.1,
            ));
        }
    };

    // 3. 鼠标松手 (吸附)
    // 🔥 核心逻辑：松手时，直接把窗口设为【最大宽度】，并定在边缘
    let window_up = window.clone();
    let handle_mouse_up = move |_| {
        if !is_dragging() {
            return;
        }
        is_dragging.set(false);
        is_hovering.set(false);

        if let Some(monitor) = window_up.current_monitor() {
            let scale = monitor.scale_factor();
            let screen_w = monitor.size().width as f64 / scale;
            let pos = window_up
                .outer_position()
                .unwrap_or(PhysicalPosition::new(0, 0));
            let x = pos.x as f64 / scale;
            let y = pos.y as f64 / scale;

            if x < screen_w / 2.0 {
                // === 左边吸附 ===
                dock_side.set(DockSide::Left);
                // 窗口 X = 0，宽度 = 140
                atomic_update_bounds(&window_up, 0.0, y, EXPANDED_W, EXPANDED_H);
            } else {
                // === 右边吸附 ===
                dock_side.set(DockSide::Right);
                // 窗口 X = Screen - 140，宽度 = 140
                // 左侧会有透明区域，鼠标穿透问题通过 "点击透明区域不响应" 虽不能完美解决但这是最稳妥的防闪烁方案
                atomic_update_bounds(&window_up, screen_w - EXPANDED_W, y, EXPANDED_W, EXPANDED_H);
            }
        }
    };

    // 4. Hover 进入
    // 🔥 不动窗口 API，只改状态触发 CSS 动画 -> 0 闪烁
    let handle_enter = move |_| {
        if is_dragging() {
            return;
        }
        if let Some(task) = debounce_task.write().take() {
            task.cancel();
        }
        is_hovering.set(true);
    };

    // 5. Hover 离开
    // 🔥 不动窗口 API，只改状态触发 CSS 动画 -> 0 闪烁
    let handle_leave = move |_| {
        if is_dragging() {
            return;
        }
        let task = spawn(async move {
            tokio::time::sleep(Duration::from_millis(300)).await;
            is_hovering.set(false);
        });
        debounce_task.set(Some(task));
    };

    // 动态类名
    let container_cls = format!(
        "dock-container {}",
        if dock_side() == DockSide::Left {
            "left"
        } else {
            "right"
        }
    );
    let capsule_cls = format!(
        "dock-capsule {} {}",
        if dock_side() == DockSide::Left {
            "left"
        } else {
            "right"
        },
        if is_hovering() { "expanded" } else { "" }
    );

    rsx! {
        div {
            class: "{container_cls}",
            onmousemove: handle_mouse_move,
            onmouseup: handle_mouse_up,

            div {
                class: "{capsule_cls}",
                onmousedown: handle_mouse_down,
                onmouseenter: handle_enter,
                onmouseleave: handle_leave,

                div { class: "capsule-content",
                    img {
                        class: "app-icon",
                        src: asset!("assets/icon.png"),
                        draggable: false,
                    }

                    div { class: "menu-area",
                        div {
                            class: if is_pinned() { "menu-btn active" } else { "menu-btn" },
                            onclick: move |evt| {
                                evt.stop_propagation();
                                is_pinned.set(!is_pinned());
                            },
                            "📌"
                        }
                        div {
                            class: "menu-btn",
                            onclick: move |evt| {
                                evt.stop_propagation();
                                if let Some(task) = debounce_task.write().take() {
                                    task.cancel();
                                }
                                window_mode.set(WindowMode::Main);
                            },
                            "💬"
                        }
                    }
                }
            }
        }
    }
}
