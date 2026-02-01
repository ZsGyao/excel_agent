use crate::models::{ChatMessage, WindowMode};
use dioxus::{
    core::{Element, Event, Task},
    desktop::{
        use_window,
        wry::dpi::{LogicalPosition, LogicalSize, PhysicalPosition},
        DesktopContext,
    },
    hooks::use_signal,
    html::{HasFileData, InteractionLocation, MouseData},
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
    let mut is_file_hovering = use_signal(|| false);
    let mut drag_start_offset = use_signal(|| (0.0, 0.0));
    let mut is_dragging = use_signal(|| false);
    let mut debounce_task = use_signal(|| None::<Task>);
    let mut anim_ready = use_signal(|| false);

    const EXPANDED_W: f64 = 130.0;
    const EXPANDED_H: f64 = 160.0;

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
            tokio::time::sleep(Duration::from_millis(150)).await;
            anim_ready.set(true);
        });
    });

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

    let handle_drag_over = move |evt: Event<DragData>| {
        evt.prevent_default();
        evt.stop_propagation();
        if !is_file_hovering() {
            is_file_hovering.set(true);
        }
    };

    let handle_drag_leave = move |evt: Event<DragData>| {
        evt.prevent_default();
        evt.stop_propagation();
        is_file_hovering.set(false);
    };

    let handle_drop = move |evt: Event<DragData>| {
        evt.prevent_default();
        evt.stop_propagation();

        is_file_hovering.set(false);

        let files = evt.data().files();
        if let Some(first_file) = files.first() {
            let file_name = first_file.name();
            let current_dir = std::env::current_dir().unwrap_or_default();
            let full_path = current_dir
                .join(&file_name)
                .to_str()
                .unwrap_or_default()
                .to_string();
            last_file_path.set(full_path);
            let new_msg_id = messages.read().len();
            messages.write().push(ChatMessage::new(
                new_msg_id,
                format!("📄 收到文件: {}", file_name),
                false,
            ));
            window_mode.set(WindowMode::Main);
        }
    };

    let container_cls = format!(
        "dock-container {}",
        if dock_side() == DockSide::Left {
            "left"
        } else {
            "right"
        }
    );
    let wrapper_cls = format!(
        "dock-wrapper {} {}",
        if is_hovering() { "expanded" } else { "" },
        if !anim_ready() { "no-anim" } else { "" }
    );
    let visibility_style = if anim_ready() {
        "opacity: 1;"
    } else {
        "opacity: 0;"
    };
    let align_style = if dock_side() == DockSide::Right {
        "align-items: flex-end;"
    } else {
        "align-items: flex-start;"
    };

    rsx! {
        div {
            class: "{container_cls}",
            style: "{visibility_style} {align_style}",
            div {
                class: "{wrapper_cls}",
                onmouseleave: handle_leave,
                ondragover: handle_drag_over,
                ondragleave: handle_drag_leave,
                ondrop: handle_drop,
                oncontextmenu: move |evt| evt.prevent_default(),

                div {
                    class: "main-capsule",
                    onmousedown: handle_mouse_down,
                    onmouseenter: handle_enter,
                    img {
                        class: "app-icon",
                        src: if is_file_hovering() { asset!("assets/get_excel.png") } else { asset!("assets/icon.png") },
                        draggable: false,
                    }
                    div {
                        class: "primary-action",
                        onclick: move |evt| {
                            evt.stop_propagation();
                            if let Some(task) = debounce_task.write().take() {
                                task.cancel();
                            }
                            window_mode.set(WindowMode::Main);
                        },
                        span { "聊天" }
                        img {
                            class: "chat-icon",
                            src: asset!("assets/chat.png"),
                            draggable: false,
                        }
                    }
                }

                div { class: "secondary-grid",
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
                        img {
                            class: "menu-icon",
                            src: asset!("assets/settings.png"),
                            draggable: false,
                        }
                    }
                    div {
                        class: if is_pinned() { "grid-btn pin active" } else { "grid-btn pin" },
                        title: "置顶",
                        onclick: move |evt| {
                            evt.stop_propagation();
                            is_pinned.set(!is_pinned());
                        },
                        img {
                            class: "menu-icon",
                            src: if is_pinned() { asset!("assets/pin_active.png") } else { asset!("assets/pin.png") },
                            draggable: false,
                        }
                    }
                    div { class: "grid-btn more", "…" }
                }
            }
        }
    }
}
