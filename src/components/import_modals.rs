use crate::models::PendingImport;
use dioxus::prelude::*;
use std::collections::HashMap;

#[component]
pub fn ImportModal(
    /// 当前待处理的导入任务信号
    pending_import: Signal<Option<PendingImport>>,
    /// 用户点击确认后的回调：传递 (文件路径, {Sheet名: 表头行数})
    on_confirm: EventHandler<(String, HashMap<String, usize>)>,
    /// 用户点击取消的回调
    on_cancel: EventHandler<()>,
) -> Element {
    // 如果当前没有待导入的任务，则不渲染任何内容
    let import_data = match pending_import.read().clone() {
        Some(data) => data,
        None => {
            return rsx! {
                div {}
            }
        }
    };

    // 局部状态：用户正在编辑的配置项
    let mut sheet_configs = use_signal(|| import_data.sheets.clone());

    let confirm_file_path = import_data.file_path.clone();

    let handle_confirm = move |_| {
        let mut config_map = HashMap::new();
        for (name, rows) in sheet_configs.read().iter() {
            config_map.insert(name.clone(), *rows);
        }
        on_confirm.call((confirm_file_path.clone(), config_map));
    };

    rsx! {
        div { class: "import-modal-overlay",
            div { class: "import-modal-container",

                // 头部
                div { class: "import-modal-header",
                    h3 { class: "import-modal-title", "📊 配置表格结构" }
                    p { class: "import-modal-subtitle", "{import_data.file_path}" }
                }

                // 内容区
                div { class: "import-modal-body custom-scrollbar",
                    p { class: "import-modal-desc",
                        "请确认每个工作表 (Sheet) 的表头行数："
                    }

                    {
                        sheet_configs
                            .read()
                            .iter()
                            .enumerate()
                            .map(|(idx, (sheet_name, rows))| {
                                let name_clone = sheet_name.clone();
                                rsx! {
                                    div { key: "{name_clone}", class: "import-modal-item",
                                        div { class: "import-modal-item-name", "📄 {sheet_name}" }
                                        div { class: "import-modal-input-group",
                                            span { "表头行:" }
                                            input {
                                                class: "import-modal-input",
                                                r#type: "number",
                                                min: "1",
                                                max: "10",
                                                value: "{rows}",
                                                oninput: move |e| {
                                                    if let Ok(val) = e.value().parse::<usize>() {
                                                        sheet_configs.write()[idx].1 = val;
                                                    }
                                                },
                                            }
                                        }
                                    }
                                }
                            })
                    }
                }

                // 底部按钮
                div { class: "import-modal-footer",
                    button {
                        class: "btn-cancel",
                        onclick: move |_| on_cancel.call(()),
                        "暂不导入"
                    }
                    button { class: "btn-confirm", onclick: handle_confirm, "🚀 确认并解析" }
                }
            }
        }
    }
}
