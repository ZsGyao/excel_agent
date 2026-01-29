use std::time::Duration;

use dioxus::{
    core::{Element, Event, Task},
    desktop::{use_window, wry::dpi::PhysicalPosition, LogicalPosition, LogicalSize},
    hooks::use_signal,
    html::{InteractionLocation, MouseData},
    prelude::*,
    signals::{Signal, WritableExt},
};

use crate::models::{ChatMessage, WindowMode};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DockSide {
    Left,
    Right,
}

#[component]
pub fn DockCapsule(
    mut window_mode: Signal<WindowMode>,
    mut messages: Signal<Vec<ChatMessage>>,
    mut last_file_path: Signal<String>,
) -> Element {
    let window = use_window();

    // State Manage
    let mut dock_side = use_signal(|| DockSide::Right);
    let mut is_pinned = use_signal(|| false);
    let mut is_hovering = use_signal(|| false);

    let mut drag_start_offset = use_signal(|| (0.0, 0.0));
    let mut is_dragging = use_signal(|| false);

    // 防抖定时器任务
    let mut debounce_task = use_signal(|| None::<Task>);

    // 常量定义
    const COLLAPSED_W: f64 = 48.0;
    const COLLAPSED_H: f64 = 56.0;
    const EXPANDED_W: f64 = 140.0;
    const EXPANDED_H: f64 = 56.0;

    // Mouse down
    let handle_mouse_down = move |evt: Event<MouseData>| {
        if is_pinned() {
            return;
        }
        let coords = evt.client_coordinates();
        drag_start_offset.set((coords.x, coords.y));
        is_dragging.set(true);
    };

    // Mouse move
    let window_move = window.clone();
    let handle_mouse_move = move |evt: Event<MouseData>| {
        if is_dragging() {
            let screen_coords = evt.screen_coordinates();
            let offset = drag_start_offset();
            window_move.set_outer_position(LogicalPosition::new(
                screen_coords.x - offset.0,
                screen_coords.y - offset.1,
            ));
        }
    };

    // Mouse up: 吸附逻辑
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

            // 强制重置大小，防止拖拽过程中尺寸异常
            window_up.set_inner_size(LogicalSize::new(COLLAPSED_W, COLLAPSED_H));

            if x < screen_w / 2.0 {
                dock_side.set(DockSide::Left);
                window_up.set_outer_position(LogicalPosition::new(0.0, y));
            } else {
                dock_side.set(DockSide::Right);
                // 🔥 绝对计算：屏幕宽度 - 收起宽度
                window_up.set_outer_position(LogicalPosition::new(screen_w - COLLAPSED_W, y));
            }
        }
    };

    // 🔥 Hover 进入：反向展开逻辑优化
    let win_enter = window.clone();
    let handle_enter = move |_| {
        if is_dragging() {
            return;
        }

        if let Some(task) = debounce_task.write().take() {
            task.cancel();
        }

        is_hovering.set(true);

        // 获取屏幕信息，进行绝对坐标计算
        if let Some(monitor) = win_enter.current_monitor() {
            let scale = monitor.scale_factor();
            let screen_w = monitor.size().width as f64 / scale;
            let pos = win_enter
                .outer_position()
                .unwrap_or(PhysicalPosition::new(0, 0));
            let current_y = pos.y as f64 / scale;

            if dock_side() == DockSide::Right {
                // 🔥 核心修复：
                // 不要用 current_x - shift，直接用 ScreenW - ExpandedW。
                // 这能保证无论之前在哪里，展开后一定紧贴右边缘，绝无缝隙。
                let target_x = screen_w - EXPANDED_W;

                // 1. 先移动位置 (把左上角移到目标点)
                win_enter.set_outer_position(LogicalPosition::new(target_x, current_y));
                // 2. 再改变大小 (向右填充)
                win_enter.set_inner_size(LogicalSize::new(EXPANDED_W, EXPANDED_H));
            } else {
                // 左侧吸附很简单，位置不变，只变大
                win_enter.set_inner_size(LogicalSize::new(EXPANDED_W, EXPANDED_H));
            }
        }
    };

    // 🔥 Hover 离开：防抖收起
    let win_leave = window.clone();
    let handle_leave = move |_| {
        if is_dragging() {
            return;
        }

        let win_async = win_leave.clone();

        let task = spawn(async move {
            tokio::time::sleep(Duration::from_millis(300)).await;

            is_hovering.set(false);

            if let Some(monitor) = win_async.current_monitor() {
                let scale = monitor.scale_factor();
                let screen_w = monitor.size().width as f64 / scale;
                let pos = win_async
                    .outer_position()
                    .unwrap_or(PhysicalPosition::new(0, 0));
                let current_y = pos.y as f64 / scale;

                if dock_side() == DockSide::Right {
                    // 🔥 核心修复：消除缩回卡顿
                    // 目标位置
                    let target_x = screen_w - COLLAPSED_W;

                    // 1. 先把位置移回去 (瞬间跳到右边)
                    win_async.set_outer_position(LogicalPosition::new(target_x, current_y));
                    // 2. 再缩小尺寸
                    win_async.set_inner_size(LogicalSize::new(COLLAPSED_W, COLLAPSED_H));
                } else {
                    win_async.set_inner_size(LogicalSize::new(COLLAPSED_W, COLLAPSED_H));
                    win_async.set_outer_position(LogicalPosition::new(0.0, current_y));
                }
            }
        });

        debounce_task.set(Some(task));
    };

    // 动态样式类
    let capsule_cls = format!(
        "dock-capsule {}",
        if dock_side() == DockSide::Left {
            "left"
        } else {
            "right"
        }
    );

    rsx! {
        div {
            class: "dock-container",
            // 使用 flex-end 确保右侧吸附时内容靠右，防止 CSS 造成的视觉缝隙
            style: if dock_side() == DockSide::Right { "justify-content: flex-end;" } else { "justify-content: flex-start;" },

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
                        src: asset!("/assets/icon.png"), // 保持你原有的写法
                        draggable: false,
                    }

                    if is_hovering() {
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
}
