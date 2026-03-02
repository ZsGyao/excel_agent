use crate::models::{ActionStatus, AppConfig, ChatMessage};
use crate::services::config::save_config;
use crate::store::app_state::use_app_state;
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use dioxus::document::eval;
use dioxus::prelude::*;
use std::collections::HashSet;
use std::path::Path;

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
    let mut state = use_app_state();
    let mut input_ref = use_signal(|| None::<std::rc::Rc<MountedData>>);

    let mut show_mention_menu = use_signal(|| false);
    let mut selected_index = use_signal(|| 0usize);
    let mut mention_level = use_signal(|| 0usize);
    let mut selected_file = use_signal(|| String::new());
    let mut selected_sheet = use_signal(|| String::new());
    let mut expanded_paths = use_signal(|| std::collections::HashSet::<String>::new());

    // 🌟 核心：监听选中索引变化，自动处理滚动条
    use_effect(move || {
        let _idx = selected_index.read(); // 订阅索引变化
        let _menu = show_mention_menu.read(); // 订阅菜单开启状态

        if *_menu {
            spawn(async move {
                // 给浏览器一点渲染时间，然后执行滚动
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                let js = r#"
                const selected = document.querySelector('.mention-item.selected');
                if (selected) {
                    selected.scrollIntoView({
                        behavior: 'smooth',
                        block: 'nearest' // 🌟 关键：只有不可见时才滚动，保持体验平滑
                    });
                }
            "#;
                let _ = eval(js);
            });
        }
    });

    // 🌟 数据计算逻辑：从真实的 global_schemas 获取数据
    let current_list = {
        let schemas = state.global_schemas.read();
        let active_paths = state.active_files.read();
        let expanded = expanded_paths.read();

        match *mention_level.read() {
            0 => active_paths
                .iter()
                .map(|p| {
                    (
                        "📄",
                        p.clone(),
                        Path::new(p).file_name().unwrap().to_string_lossy().into(),
                        0,
                        true,
                    )
                })
                .collect(),
            1 => {
                let file = selected_file.read();
                schemas
                    .get(&*file)
                    .map(|fs| {
                        fs.sheets
                            .keys()
                            .map(|s| ("📑", s.clone(), s.clone(), 0, true))
                            .collect()
                    })
                    .unwrap_or_default()
            }
            2 => {
                let file = selected_file.read();
                let sheet = selected_sheet.read();
                schemas
                    .get(&*file)
                    .and_then(|fs| fs.sheets.get(&*sheet))
                    .map(|ss| {
                        let mut items = Vec::new();
                        let mut seen_nodes = HashSet::new();

                        for col in &ss.columns {
                            let full_path = &col.semantic_name;
                            let parts: Vec<&str> = full_path.split("@|||@").collect();
                            let mut current_path = String::new();

                            for (i, part) in parts.iter().enumerate() {
                                let parent_path = current_path.clone();
                                if !current_path.is_empty() {
                                    current_path.push_str("@|||@");
                                }
                                current_path.push_str(part);

                                // 🌟 过滤逻辑：第一层必显，非第一层需父级已展开
                                let should_show = i == 0 || expanded.contains(&parent_path);

                                if should_show && seen_nodes.insert(current_path.clone()) {
                                    let is_leaf = i == parts.len() - 1;
                                    let indent = i * 16;
                                    let is_open = expanded.contains(&current_path);
                                    let icon = if is_leaf {
                                        "🏷️"
                                    } else if is_open {
                                        "📂"
                                    } else {
                                        "📁"
                                    };
                                    let clean_part = part.replace('\n', " ").replace('\r', "");
                                    let display = if is_leaf {
                                        clean_part.clone()
                                    } else {
                                        format!("{}", clean_part)
                                    };

                                    // 元组结构：(图标, 路径, 显示名, 缩进, 是否叶子)
                                    items.push((
                                        icon,
                                        current_path.clone(),
                                        display,
                                        indent,
                                        is_leaf,
                                    ));
                                }
                            }
                        }
                        items
                    })
                    .unwrap_or_default()
            }
            _ => vec![],
        }
    };

    let current_list_len = current_list.len();

    // 🌟 消息请求封装
    let mut perform_request = move |prompt_text: String, is_auto_fix: bool| {
        // 不要在这里直接 is_loading.set(false)，因为 AI 还没开始跑
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

        // 这里的 call 会触发 chat_controller 里的逻辑
        on_run_code.call(ai_id);
        // 注意：is_loading 的关闭应该由具体执行代码的协程负责，这里先保持 true
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

    // 定义一个切换折叠的函数
    let mut toggle_folder = move |path: String| {
        let mut expanded = expanded_paths.write();
        if expanded.contains(&path) {
            expanded.remove(&path);
        } else {
            expanded.insert(path);
        }
    };

    // 🌟 胶囊植入逻辑
    let mut insert_pill_fn = move |idx: usize,
                                   list: Vec<(&'static str, String, String, usize, bool)>,
                                   force_insert: bool| {
        if idx >= list.len() {
            return;
        }
        let (icon, actual_val, display_name, _, is_leaf) = list[idx].clone();

        // 🌟 修复点 1：如果是文件夹，且没有按 Shift (force_insert=false)，才执行折叠
        // 如果按了 Shift，就会跳过这个 if，继续往下生成胶囊！
        if !is_leaf && !force_insert {
            let mut expanded = expanded_paths.write();
            if expanded.contains(&actual_val) {
                expanded.remove(&actual_val);
            } else {
                expanded.insert(actual_val);
            }
            return;
        }

        // --- 叶子节点或强行插入分类的逻辑 ---
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

        // 🌟 终极护甲：全部 Base64 编码，彻底断绝转义符和换行符带来的灾难！
        let file_b64 = B64.encode(&file);
        let sheet_b64 = B64.encode(&sheet);
        let col_b64 = B64.encode(&col);

        let raw_pill_text = format!("{} {}", icon, display_name);
        // UI 显示文本，仅仅为了不撑破输入框，把换行替换成空格，单引号转义
        let safe_pill_text = raw_pill_text
            .replace('\n', " ")
            .replace('\r', "")
            .replace('\'', "\\'");

        let theme_class = if lvl == 0 {
            "pill-file"
        } else if lvl == 1 {
            "pill-sheet"
        } else if lvl == 2 && !is_leaf {
            "pill-category"
        } else {
            "pill-column"
        };
        let full_pill_class = format!("pill-base {}", theme_class);

        // 🌟 现在注入到 JS 里的全都是安全的大小写字母，绝对不可能报错了
        let js = format!(
            r#"
            let sel = window.getSelection();
            if (sel.rangeCount > 0) {{
                let range = sel.getRangeAt(0);
                let node = range.startContainer;
                
                let span = document.createElement('span');
                span.className = '{}';
                span.contentEditable = 'false'; 
                
                // 存入的是安全的 Base64 字符串
                span.setAttribute('data-file', '{}'); 
                span.setAttribute('data-sheet', '{}'); 
                span.setAttribute('data-col', '{}'); 
                span.innerText = '{}';

                if (node.nodeType === Node.TEXT_NODE) {{
                    let text = node.textContent;
                    let offset = range.startOffset;
                    let atIdx = Math.max(text.substring(0, offset).lastIndexOf('@'), text.substring(0, offset).lastIndexOf('＠'));
                    if (atIdx !== -1) {{
                        range.setStart(node, atIdx); range.deleteContents();
                    }} else {{ range.deleteContents(); }}
                }} else {{ range.deleteContents(); }}
                
                range.insertNode(span);
                let space = document.createTextNode('\u00A0'); 
                span.parentNode.insertBefore(space, span.nextSibling);
                range.setStartAfter(space); range.collapse(true);
                sel.removeAllRanges(); sel.addRange(range);
            }}
            "#,
            full_pill_class, file_b64, sheet_b64, col_b64, safe_pill_text
        );

        spawn(async move {
            let _ = eval(&js);
        });
        show_mention_menu.set(false);
    };

    // 🌟 发送按钮逻辑修复：改用 dioxus.send
    let mut extract_and_send = move || {
        if is_loading() {
            return;
        }
        show_mention_menu.set(false); // 发送时自动关闭菜单

        spawn(async move {
            let js_code = r#"
                let container = document.getElementById("rich-chat-input");
                
                // 🌟 架构升级：构建标准的结构化 API 请求体
                let payload = {
                    raw_query: "",
                    mentions: []
                };
                
                if (container) {
                    let mention_index = 0;
                    for (let node of container.childNodes) {
                        if (node.nodeType === Node.TEXT_NODE) {
                            payload.raw_query += node.textContent;
                        } else if (node.nodeType === Node.ELEMENT_NODE && node.hasAttribute('data-file')) {
                            // 遇到胶囊，生成一个占位符，比如 {{REF_0}}
                            let placeholder = `{{REF_${mention_index}}}`;
                            payload.raw_query += placeholder;
                            
                            // 收集真实元数据
                            payload.mentions.push({
                                placeholder: placeholder,
                                file: node.getAttribute('data-file') || "",
                                sheet: node.getAttribute('data-sheet') || "",
                                col: node.getAttribute('data-col') || ""
                            });
                            mention_index++;
                        } else if (node.nodeType === Node.ELEMENT_NODE) {
                            payload.raw_query += node.innerText;
                        }
                    }
                }
                
                // 转化为 JSON 字符串发送给 Rust
                dioxus.send(JSON.stringify(payload));
            "#;

            let mut eval_handle = eval(js_code);
            if let Ok(json_val) = eval_handle.recv::<serde_json::Value>().await {
                if let Some(json_str) = json_val.as_str() {
                    if !json_str.trim().is_empty()
                        && json_str != r#"{"raw_query":"","mentions":[]}"#
                    {
                        // 将整个 JSON 字符串发给后端（后端再解析）
                        perform_request(json_str.to_string(), false);
                        let _ = eval("document.getElementById('rich-chat-input').innerHTML = '';");
                    }
                }
            }
        });
    };

    // 原有模型切换逻辑
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

    // 🌟 键盘劫持处理 (保持原样，修复了 set 调用)
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
                    if idx < current_list_len {
                        // 🌟 修复：解构 5 个元素
                        let (_, val, _, _, is_leaf) = &current_list_for_kbd[idx];
                        if lvl < 2 {
                            if lvl == 0 {
                                selected_file.set(val.clone());
                            }
                            if lvl == 1 {
                                selected_sheet.set(val.clone());
                            }
                            mention_level.set(lvl + 1);
                            selected_index.set(0);
                        } else if !is_leaf {
                            // 层级 2 文件夹，向右键展开
                            expanded_paths.write().insert(val.clone());
                        }
                    }
                }
                Key::Enter => {
                    evt.prevent_default();
                    let lvl = *mention_level.read();
                    let idx = *selected_index.read();
                    if idx < current_list_len {
                        let (_, val, _, _, is_leaf) = &current_list_for_kbd[idx];

                        // 🌟 提取 Shift 按键状态
                        let is_shift = evt.modifiers().contains(Modifiers::SHIFT);

                        // 🌟 修复点 4：把 is_shift 传给 insert_pill_fn
                        if is_shift || (lvl == 2 && *is_leaf) {
                            insert_pill_fn(idx, current_list_for_kbd.clone(), is_shift);
                        } else if lvl < 2 {
                            if lvl == 0 {
                                selected_file.set(val.clone());
                            }
                            if lvl == 1 {
                                selected_sheet.set(val.clone());
                            }
                            mention_level.set(lvl + 1);
                            selected_index.set(0);
                        } else if !is_leaf {
                            // 层级 2 文件夹，回车切换折叠
                            let mut expanded = expanded_paths.write();
                            if expanded.contains(val) {
                                expanded.remove(val);
                            } else {
                                expanded.insert(val.clone());
                            }
                        }
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
                let js = "let txt = document.getElementById('rich-chat-input').textContent || ''; dioxus.send(txt);";
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

            div { class: "hidden bg-purple-100 text-purple-800 border-purple-200 bg-emerald-100 text-emerald-800 border-emerald-200 bg-amber-100 text-amber-800 border-amber-200 bg-blue-100 text-blue-800 border-blue-200 shadow-sm rounded-md border" }

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
                        div {
                            class: "mention-popover-body",
                            style: "max-height: 300px; overflow-y: auto;",
                            for (idx , item) in current_list.clone().into_iter().enumerate() {
                                {
                                    let (icon, path, display, indent, is_leaf) = item;
                                    let is_selected = *selected_index.read() == idx;
                                    let is_open = expanded_paths.read().contains(&path);
                                    let list_for_click = current_list.clone();

                                    rsx! {
                                        button {
                                            key: "{path}",
                                            class: if is_selected { "mention-item selected" } else { "mention-item" },
                                            // 🌟 动态计算缩进并应用样式
                                            style: "padding-left: {indent + 12}px; display: flex; align-items: center; width: 100%; text-align: left;",
                                            onmousedown: move |e| e.prevent_default(),
                                            onclick: move |e| {
                                                let is_shift = e.modifiers().contains(Modifiers::SHIFT);
                                                insert_pill_fn(idx, list_for_click.clone(), is_shift)
                                            },
                                            onmouseenter: move |_| selected_index.set(idx),

                                            // 🌟 文件夹节点增加展开/收起小箭头
                                            if !is_leaf {
                                                span {
                                                    class: "mr-1 text-[10px] transition-transform duration-200",
                                                    style: if is_open { "transform: rotate(90deg);" } else { "" },
                                                    "▶"

                                                }
                                            } else {
                                                // 叶子节点占位，保持对齐
                                                span { class: "w-3" }
                                            }

                                            span { class: "icon mr-1.5", "{icon}" }
                                            span {
                                                class: "text truncate",
                                                style: if !is_leaf { "font-weight: 600; color: #444;" } else { "" },
                                                "{display}"
                                            }
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
