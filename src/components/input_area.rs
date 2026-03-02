use crate::models::{ActionStatus, AppConfig, ChatMessage};
use crate::services::config::save_config;
use dioxus::document::eval;
use dioxus::prelude::*;

#[component]
pub fn InputArea(
    messages: Signal<Vec<ChatMessage>>,
    active_files: Signal<Vec<String>>,
    is_loading: Signal<bool>,
    config: Signal<AppConfig>,
    error_fix_signal: Signal<Option<String>>,
    on_run_code: EventHandler<usize>,
    on_open_file: EventHandler<()>,
) -> Element {
    let mut input_ref = use_signal(|| None::<std::rc::Rc<MountedData>>);

    let mut show_mention_menu = use_signal(|| false);
    let mut selected_index = use_signal(|| 0usize);
    let mut mention_level = use_signal(|| 0usize);
    let mut selected_file = use_signal(|| String::new());
    let mut selected_sheet = use_signal(|| String::new());

    // 🌟 模拟数据保持不变
    let mock_schema = vec![
        (
            "2026年度考核.xlsx",
            vec![
                (
                    "员工考核表",
                    vec![
                        "基本信息@|||@姓名",
                        "基本信息@|||@部门",
                        "2026年度大考@|||@理论成绩",
                        "2026年度大考@|||@实操成绩",
                    ],
                ),
                ("部门映射表", vec!["部门", "负责人", "所在楼层"]),
            ],
        ),
        (
            "系统配置表.xlsx",
            vec![("Sheet1", vec!["配置项", "数值", "状态"])],
        ),
    ];

    // 💡 动态计算列表 (这里使用引用，不移动所有权)
    let current_list = match *mention_level.read() {
        0 => mock_schema
            .iter()
            .map(|(f, _)| ("📄", f.to_string(), f.to_string()))
            .collect::<Vec<_>>(),
        1 => {
            let file = selected_file.read();
            mock_schema
                .iter()
                .find(|(f, _)| f == &*file)
                .map(|(_, sheets)| {
                    sheets
                        .iter()
                        .map(|(s, _)| ("📑", s.to_string(), s.to_string()))
                        .collect()
                })
                .unwrap_or_default()
        }
        2 => {
            let file = selected_file.read();
            let sheet = selected_sheet.read();
            mock_schema
                .iter()
                .find(|(f, _)| f == &*file)
                .and_then(|(_, sheets)| sheets.iter().find(|(s, _)| s == &*sheet))
                .map(|(_, cols)| {
                    cols.iter()
                        .map(|c| {
                            let short_name = c.split("@|||@").last().unwrap_or(c).to_string();
                            ("🏷️", c.to_string(), short_name)
                        })
                        .collect()
                })
                .unwrap_or_default()
        }
        _ => vec![],
    };

    let current_list_len = current_list.len();

    // 🌟 业务逻辑封装
    let mut perform_request = move |prompt_text: String, is_auto_fix: bool| {
        if is_loading() {
            return;
        }
        is_loading.set(true);
        let user_id = messages.read().len();
        let display = if is_auto_fix {
            format!("自动修复: {}", prompt_text)
        } else {
            prompt_text
        };
        messages
            .write()
            .push(ChatMessage::new(user_id, &display, true));
        let ai_id = messages.read().len();
        let mut ai_msg = ChatMessage::loading(ai_id);
        ai_msg.status = ActionStatus::Running;
        messages.write().push(ai_msg);
        on_run_code.call(ai_id);
        is_loading.set(false);
    };

    use_effect(move || {
        if let Some(err) = error_fix_signal() {
            let err_clone = err.clone();
            spawn(async move {
                error_fix_signal.set(None);
                perform_request(err_clone, true);
            });
        }
    });

    // 🌟 核心：插入胶囊逻辑封装在一个统一的函数中
    // 接受 list 作为参数，避免闭包捕获所有权问题
    let mut insert_pill_fn = move |idx: usize, list: Vec<(&'static str, String, String)>| {
        if idx >= list.len() {
            return;
        }
        let (icon, actual_val, display_name) = list[idx].clone();
        let lvl = *mention_level.read();
        let file = if lvl == 0 {
            actual_val.clone()
        } else {
            selected_file.read().clone()
        };
        let sheet = if lvl == 1 {
            actual_val.clone()
        } else if lvl > 1 {
            selected_sheet.read().clone()
        } else {
            "".to_string()
        };
        let col = if lvl == 2 {
            actual_val.clone()
        } else {
            "".to_string()
        };

        let ref_tag = format!("[[REF:{}|{}|{}]]", file, sheet, col);
        let pill_text = format!("{} {}", icon, display_name);

        let js = format!(
            r#"
            let sel = window.getSelection();
            if (sel.rangeCount > 0) {{
                let range = sel.getRangeAt(0);
                let node = range.startContainer;
                if (node.nodeType === Node.TEXT_NODE) {{
                    let text = node.textContent;
                    let cursorOffset = range.startOffset;
                    let atIndex = Math.max(text.substring(0, cursorOffset).lastIndexOf('@'), text.substring(0, cursorOffset).lastIndexOf('＠'));
                    if (atIndex !== -1) {{
                        range.setStart(node, atIndex); range.deleteContents();
                        let span = document.createElement('span');
                        span.className = 'inline-flex items-center px-2 py-0.5 mx-1 rounded text-xs font-medium bg-blue-100 text-blue-800 select-none cursor-default';
                        span.contentEditable = 'false'; span.setAttribute('data-ref', '{}'); span.innerText = '{}';
                        range.insertNode(span);
                        let space = document.createTextNode('\u00A0'); span.parentNode.insertBefore(space, span.nextSibling);
                        range.setStartAfter(space); range.collapse(true);
                        sel.removeAllRanges(); sel.addRange(range);
                    }}
                }}
            }}
        "#,
            ref_tag, pill_text
        );

        spawn(async move {
            let _ = eval(&js).recv::<serde_json::Value>().await;
        });
        show_mention_menu.set(false);
    };

    let mut switch_model = move |_| {
        let mut cfg = config.read().clone();
        if cfg.profiles.is_empty() {
            return;
        }
        let current_idx = cfg
            .profiles
            .iter()
            .position(|p| Some(&p.id) == cfg.active_profile_id.as_ref())
            .unwrap_or(0);
        cfg.active_profile_id = Some(
            cfg.profiles[(current_idx + 1) % cfg.profiles.len()]
                .id
                .clone(),
        );
        config.set(cfg.clone());
        save_config(&cfg);
    };

    let active_model_name = config.read().active_profile().name.clone();

    // 发送提取逻辑
    let mut extract_and_send = move || {
        if is_loading() {
            return;
        }
        spawn(async move {
            let js_code = r#"
                let container = document.getElementById("rich-chat-input");
                let payload = "";
                for (let node of container.childNodes) {
                    if (node.nodeType === Node.TEXT_NODE) payload += node.textContent;
                    else if (node.nodeType === Node.ELEMENT_NODE) payload += node.hasAttribute('data-ref') ? node.getAttribute('data-ref') : node.innerText;
                }
                return payload.trim();
            "#;
            if let Ok(json_val) = eval(js_code).recv::<serde_json::Value>().await {
                if let Some(payload) = json_val.as_str() {
                    if !payload.is_empty() {
                        perform_request(payload.to_string(), false);
                        let _ = eval("document.getElementById('rich-chat-input').innerHTML = '';")
                            .recv::<serde_json::Value>()
                            .await;
                    }
                }
            }
        });
    };

    // 🌟 键盘劫持处理
    let current_list_for_kbd = current_list.clone();
    let mut handle_keydown = move |evt: Event<KeyboardData>| {
        if *show_mention_menu.read() {
            match evt.key() {
                Key::ArrowDown => {
                    evt.prevent_default();
                    let cur = *selected_index.read();
                    if current_list_len > 0 {
                        selected_index.set((cur + 1).min(current_list_len - 1));
                    }
                }
                Key::ArrowUp => {
                    evt.prevent_default();
                    let cur = *selected_index.read();
                    selected_index.set(cur.saturating_sub(1));
                }
                Key::ArrowLeft => {
                    evt.prevent_default();
                    let lvl = *mention_level.read();
                    if lvl > 0 {
                        mention_level.set(lvl - 1);
                        selected_index.set(0);
                    }
                }
                Key::ArrowRight => {
                    evt.prevent_default();
                    let lvl = *mention_level.read();
                    let idx = *selected_index.read();
                    if lvl < 2 && idx < current_list_len {
                        let (_, val, _) = &current_list_for_kbd[idx];
                        if lvl == 0 {
                            selected_file.set(val.clone());
                        }
                        if lvl == 1 {
                            selected_sheet.set(val.clone());
                        }
                        mention_level.set(lvl + 1);
                        selected_index.set(0);
                    }
                }
                Key::Enter => {
                    evt.prevent_default();
                    let lvl = *mention_level.read();
                    let idx = *selected_index.read();
                    if evt.modifiers().contains(Modifiers::SHIFT) || lvl == 2 {
                        insert_pill_fn(idx, current_list_for_kbd.clone());
                    } else if idx < current_list_len {
                        let (_, val, _) = &current_list_for_kbd[idx];
                        if lvl == 0 {
                            selected_file.set(val.clone());
                        }
                        if lvl == 1 {
                            selected_sheet.set(val.clone());
                        }
                        mention_level.set(lvl + 1);
                        selected_index.set(0);
                    }
                }
                Key::Escape => {
                    show_mention_menu.set(false);
                }
                _ => {}
            }
            return;
        }
        if evt.key() == Key::Enter && !evt.modifiers().contains(Modifiers::SHIFT) {
            evt.prevent_default();
            extract_and_send();
        }
    };

    let mut handle_keyup = move |evt: Event<KeyboardData>| {
        let key_str = evt.key().to_string();
        if key_str == "@" || key_str == "＠" {
            show_mention_menu.set(true);
            mention_level.set(0);
            selected_index.set(0);
        }
        if *show_mention_menu.read() {
            let mut menu_state = show_mention_menu.clone();
            spawn(async move {
                let js = "return document.getElementById('rich-chat-input').textContent || '';";
                if let Ok(val) = eval(js).recv::<serde_json::Value>().await {
                    if let Some(text) = val.as_str() {
                        if !text.contains('@') && !text.contains('＠') {
                            menu_state.set(false);
                        }
                    }
                }
            });
        }
    };

    let header_title = match *mention_level.read() {
        0 => "选择关联文件 (Shift+Enter 选中, → 下钻)",
        1 => "选择工作表 (Shift+Enter 选中, ← 返回, → 下钻)",
        _ => "选择特定数据列 (Enter 选中, ← 返回)",
    };

    rsx! {
        div { class: "input-container",
            div { class: "input-toolbar",
                div { class: "model-selector", onclick: switch_model, "{active_model_name} ▾" }
                button {
                    class: "tool-btn",
                    onclick: move |_| on_open_file.call(()),
                    "📎"
                }
            }

            div { class: "input-wrapper relative w-full flex flex-col",
                if *show_mention_menu.read() {
                    div { class: "mention-popover",
                        div { class: "mention-popover-header", "{header_title}" }
                        div { class: "mention-popover-body",
                            // 这里克隆 current_list 用于循环渲染
                            for (idx , item) in current_list.clone().into_iter().enumerate() {
                                {
                                    let (icon, _, display) = item;
                                    let is_selected = *selected_index.read() == idx;
                                    let list_for_click = current_list.clone();
                                    rsx! {
                                        button {
                                            class: if is_selected { "mention-item selected" } else { "mention-item" },
                                            onmousedown: move |e| e.prevent_default(),
                                            onclick: move |_| {
                                                insert_pill_fn(idx, list_for_click.clone());
                                            },
                                            onmouseenter: move |_| selected_index.set(idx),
                                            span { class: "icon", "{icon}" }
                                            span { class: "text", "{display}" }
                                        }
                                    }
                                }
                            }
                            if current_list_len == 0 {
                                div { class: "px-4 py-3 text-xs text-gray-500", "无可选项" }
                            }
                        }
                    }
                }

                div {
                    id: "rich-chat-input",
                    onmounted: move |cx| input_ref.set(Some(cx.data())),
                    class: "chat-input w-full min-h-[60px] max-h-[200px] overflow-y-auto p-3 outline-none text-sm text-gray-800 leading-relaxed",
                    contenteditable: "true",
                    "data-placeholder": "输入指令，或输入 @ 引用具体的表格列...",
                    onkeydown: handle_keydown,
                    onkeyup: handle_keyup,
                }

                button {
                    class: "send-btn absolute bottom-3 right-3",
                    disabled: is_loading(),
                    onclick: move |_| extract_and_send(),
                    if is_loading() {
                        "..."
                    } else {
                        "⬆"
                    }
                }
            }
        }
    }
}
