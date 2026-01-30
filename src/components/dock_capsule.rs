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

use crate::models::{ChatMessage, WindowMode};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DockSide {
    Left,
    Right,
}

// 获取 HWND (Windows 句柄)
#[cfg(target_os = "windows")]
fn get_hwnd(window: &DesktopContext) -> Option<windows_sys::Win32::Foundation::HWND> {
    use windows_sys::Win32::Foundation::HWND;
    if let Ok(handle) = window.window_handle() {
        if let RawWindowHandle::Win32(win32_handle) = handle.as_raw() {
            let hwnd_isize = win32_handle.hwnd.get();
            return Some(hwnd_isize as HWND);
        }
    }
    None
}

// 原子更新函数 (SetWindowPos)
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
                    SWP_NOZORDER | SWP_NOACTIVATE,
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

    // 默认为 false，这会给组件加上 .no-anim 类，禁止一切过渡效果
    let mut anim_ready = use_signal(|| false);

    const EXPANDED_W: f64 = 120.0;
    const EXPANDED_H: f64 = 56.0;

    // 🔥 核心修复：组件挂载后，延迟一小会儿再开启动画
    // 这样初次渲染（从 Main 切回来时）就是瞬间完成的，不会有缩放过程
    let window_init = window.clone();
    use_effect(move || {
        // 初始化时检测窗口位置，决定是在左边还是右边
        if let Some(monitor) = window_init.current_monitor() {
            let scale = monitor.scale_factor();
            let screen_w = monitor.size().width as f64 / scale;

            // 获取当前窗口位置
            if let Ok(pos) = window_init.outer_position() {
                let x = pos.x as f64 / scale;

                // 如果 X 坐标小于屏幕一半，说明在左边
                if x < screen_w / 2.0 {
                    dock_side.set(DockSide::Left);
                } else {
                    dock_side.set(DockSide::Right);
                }
            }
        }

        spawn(async move {
            // 50ms 足够浏览器完成初次绘制布局了
            tokio::time::sleep(Duration::from_millis(100)).await;
            anim_ready.set(true);
        });
    });

    // 🔥 监听 is_dragging 状态的副作用
    let window_drag_loop = window.clone();
    use_effect(move || {
        if is_dragging() {
            // 🔥 关键修复：在这里再次 Clone！
            // 这样每次副作用运行时，都会生成一个新的句柄给 async 任务，
            // 而不是试图把外部的 window_drag_loop 变量“吃掉”。
            let window_async = window_drag_loop.clone();

            spawn(async move {
                loop {
                    // 1. 检查鼠标左键是否还按着
                    #[cfg(target_os = "windows")]
                    {
                        use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
                            GetAsyncKeyState, VK_LBUTTON,
                        };
                        unsafe {
                            let state = GetAsyncKeyState(VK_LBUTTON as i32);
                            if (state as u16 & 0x8000) == 0 {
                                is_dragging.set(false);
                                break;
                            }
                        }
                    }

                    // 2. 获取全局鼠标位置
                    #[cfg(target_os = "windows")]
                    {
                        use windows_sys::Win32::Foundation::POINT;
                        use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;

                        let mut point = POINT { x: 0, y: 0 };
                        unsafe { GetCursorPos(&mut point) };

                        if let Some(monitor) = window_async.current_monitor() {
                            let scale = monitor.scale_factor();
                            let offset = drag_start_offset();

                            let mouse_x_logical = point.x as f64 / scale;
                            let mouse_y_logical = point.y as f64 / scale;

                            let new_x = mouse_x_logical - offset.0;
                            let new_y = mouse_y_logical - offset.1;

                            // 使用 window_async 进行移动
                            atomic_update_bounds(
                                &window_async,
                                new_x,
                                new_y,
                                EXPANDED_W,
                                EXPANDED_H,
                            );
                        }
                    }

                    tokio::time::sleep(Duration::from_millis(8)).await;
                }

                // === 循环结束，鼠标松开 ===
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

    // 2. 鼠标进入
    let handle_enter = move |_| {
        if is_dragging() {
            return;
        }
        if let Some(task) = debounce_task.write().take() {
            task.cancel();
        }
        is_hovering.set(true);
    };

    // 3. 鼠标离开
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
    // 🔥 动态添加 .no-anim 类
    let capsule_cls = format!(
        "dock-capsule {} {} {}",
        if dock_side() == DockSide::Left {
            "left"
        } else {
            "right"
        },
        if is_hovering() { "expanded" } else { "" },
        if !anim_ready() { "no-anim" } else { "" } // 刚加载时禁用动画
    );

    rsx! {
        div {
            class: "{container_cls}",
            style: if dock_side() == DockSide::Right { "justify-content: flex-end;" } else { "justify-content: flex-start;" },

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

                    // 🔥 菜单区域
                    div { class: "menu-area",
                        // 按钮 1: 聊天 (左上)
                        div {
                            class: "grid-btn chat",
                            title: "聊天",
                            onclick: move |evt| {
                                evt.stop_propagation();
                                if let Some(task) = debounce_task.write().take() {
                                    task.cancel();
                                }
                                window_mode.set(WindowMode::Main);
                            },
                            "💬"
                        }

                        // 按钮 2: 设置 (右上)
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

                        // 按钮 3: 置顶 (左下)
                        div {
                            class: if is_pinned() { "grid-btn pin active" } else { "grid-btn pin" },
                            title: "置顶",
                            onclick: move |evt| {
                                evt.stop_propagation();
                                is_pinned.set(!is_pinned());
                            },
                            "📌"
                        }

                        // 按钮 4: 预留/占位 (右下) - 比如未来放 "历史记录"
                        div { class: "grid-btn more", title: "更多",
                            // 暂时没功能，放个点点点
                            "…"
                        }
                    }
                }
            }
        }
    }
}
