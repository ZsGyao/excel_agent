use dioxus::{
    core::{Element, Event},
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

pub fn DockCapsule(
    mut window_mode: Signal<WindowMode>,
    mut message: Signal<Vec<ChatMessage>>,
    mut last_file_path: Signal<String>,
) -> Element {
    let window = use_window();

    // State Manage
    let mut dock_side = use_signal(|| DockSide::Right);
    let mut is_pinned = use_signal(|| false);
    let mut is_hovering = use_signal(|| false);

    // Records the offset when the mouse is pressed for custom dragging
    let mut drag_start_offset = use_signal(|| (0.0, 0.0));
    let mut is_dragging = use_signal(|| false);

    // Mouse down, start drag
    let handle_mouse_down = move |evt: Event<MouseData>| {
        if is_pinned() {
            return;
        } // If pinned, cannot drag

        let coords = evt.client_coordinates();
        drag_start_offset.set((coords.x, coords.y));
        is_dragging.set(true);
    };

    // Mouse move, update window position
    let window_move = window.clone();
    let handle_mouse_move = move |evt: Event<MouseData>| {
        if is_dragging() {
            let screen_coords = evt.screen_coordinates();
            let offset = drag_start_offset();
            // move window direct
            window_move.set_outer_position(LogicalPosition::new(
                screen_coords.x - offset.0,
                screen_coords.y - offset.1,
            ));
        }
    };

    // Mouse up, calculate snap logical
    let window_up = window.clone();
    let handle_mouse_up = move |_| {
        if !is_dragging() {
            return;
        }
        is_dragging.set(false);

        // Get screen info
        if let Some(monitor) = window_up.current_monitor() {
            let screen_size = monitor.size();
            let scale = monitor.scale_factor();
            let screen_width = screen_size.width as f64 / scale;

            let win_pos = window_up
                .outer_position()
                .unwrap_or(PhysicalPosition::new(0, 0));
            let win_x = win_pos.x as f64 / scale;
            let win_y = win_pos.y as f64 / scale;

            // 核心吸附逻辑
            // 胶囊宽度假设 40px
            if win_x < screen_width / 2.0 {
                // 吸附到左边
                dock_side.set(DockSide::Left);
                window_up.set_outer_position(LogicalPosition::new(0.0, win_y));
            } else {
                // 吸附到右边
                dock_side.set(DockSide::Right);
                window_up.set_outer_position(LogicalPosition::new(screen_width - 40.0, win_y));
            }
        }
    };

    // 动态计算样式类
    let container_class = format!(
        "dock-capsule {} {}",
        if dock_side() == DockSide::Left {
            "left"
        } else {
            "right"
        },
        if is_hovering() { "expanded" } else { "" } // Hover 时变宽
    );

    let window_enter = window.clone();
    let window_leave = window.clone();
    rsx! {
        div {
            class: "dock-container",
            onmousemove: handle_mouse_move,
            onmouseup: handle_mouse_up,

            div {
                class: "{container_class}",
                onmousedown: handle_mouse_down,

                // Hover 展开逻辑
                onmouseenter: move |_| {
                    if !is_dragging() {
                        window_enter.set_inner_size(LogicalSize::new(180.0, 60.0));

                        // 如果在右边，为了视觉体验，可能需要调整位置(向左伸展)
                        // 这里暂时不做复杂位移，CSS flex-direction: row-reverse 已经处理了内容排列
                        is_hovering.set(true);
                    }
                },

                // Hover 离开：收缩并吸附
                onmouseleave: move |_| {
                    if !is_dragging() {
                        is_hovering.set(false);
                        window_leave.set_inner_size(LogicalSize::new(40.0, 60.0));

                        // 🔥 修复：手动复制吸附逻辑，而不是调用 handle_mouse_up(())
                        // 确保收起时贴紧边缘
                        if let Some(monitor) = window_leave.current_monitor() {
                            let screen_size = monitor.size();
                            let scale = monitor.scale_factor();
                            let screen_width = screen_size.width as f64 / scale;
                            let win_pos = window_leave
                                .outer_position()
                                .unwrap_or(PhysicalPosition::new(0, 0));
                            let win_y = win_pos.y as f64 / scale;
                            let win_x = win_pos.x as f64 / scale;

                            if win_x < screen_width / 2.0 {
                                window_leave.set_outer_position(LogicalPosition::new(0.0, win_y));
                            } else {
                                window_leave
                                    .set_outer_position(
                                        LogicalPosition::new(screen_width - 40.0, win_y),
                                    );
                            }
                        }
                    }
                },

                // Logo
                div {
                    class: "paw-logo",
                    style: "background-image: url('assets/icon.jpg');",
                }

                // 菜单区
                if is_hovering() {
                    div { class: "menu-items",
                        div {
                            class: if is_pinned() { "menu-icon pinned" } else { "menu-icon" },
                            onclick: move |evt| {
                                evt.stop_propagation();
                                is_pinned.set(!is_pinned());
                            },
                            title: "固定",
                            "📌"
                        }
                        div { class: "menu-icon", "⚙️" }
                        div {
                            class: "menu-icon main-btn",
                            onclick: move |evt| {
                                evt.stop_propagation();
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
