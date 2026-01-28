use crate::components::data_table::DataTable;
use crate::models::{ActionStatus, ChatMessage}; // ✅ 引入 ActionStatus
use crate::services::python; // ✅ 引入 python 服务
use dioxus::prelude::*;
use tokio::task; // ✅ 引入 task

#[component]
pub fn ChatView(messages: Signal<Vec<ChatMessage>>, last_file_path: Signal<String>) -> Element {
    // 处理点击确认
    let handle_confirm = move |msg_id: usize, temp_id: String| {
        let path = last_file_path.read().clone();
        spawn(async move {
            let result = task::spawn_blocking(move || python::confirm_save(&path, &temp_id))
                .await
                .unwrap_or("❌ 线程错误".to_string());

            let mut msgs = messages.write();
            if let Some(msg) = msgs.iter_mut().find(|m| m.id == msg_id) {
                msg.status = ActionStatus::Confirmed;
                msg.text = format!("{}\n\n{}", msg.text, result);
            }
        });
    };

    // 处理点击放弃
    let handle_discard = move |msg_id: usize, temp_id: String| {
        let path = last_file_path.read().clone();
        spawn(async move {
            let _ = task::spawn_blocking(move || python::discard_change(&path, &temp_id)).await;

            let mut msgs = messages.write();
            if let Some(msg) = msgs.iter_mut().find(|m| m.id == msg_id) {
                msg.status = ActionStatus::Discarded;
                msg.text = format!("{}\n\n(已放弃修改)", msg.text);
                msg.table = None;
            }
        });
    };

    rsx! {
        div { class: "chat-scroll",
            for msg in messages.read().iter() {
                div {
                    key: "{msg.id}",
                    class: if msg.is_user { "message msg-user" } else { "message msg-ai" },

                    div { style: "white-space: pre-wrap;", "{msg.text}" }

                    if let Some(table_data) = &msg.table {
                        DataTable { data: table_data.clone() }
                    }

                    if msg.status == ActionStatus::Pending {
                        if let Some(temp_id) = &msg.temp_id {
                            // ✅ 修复点：用 {} 包裹代码块，然后再返回 rsx!
                            {
                                let t_id_confirm = temp_id.clone();
                                let t_id_discard = temp_id.clone();
                                let m_id = msg.id;

                                rsx! {
                                    div { style: "margin-top: 10px; display: flex; gap: 10px;",
                                        button {
                                            class: "btn-confirm",
                                            onclick: move |_| handle_confirm(m_id, t_id_confirm.clone()),
                                            "✅ 确认生效"
                                        }
                                        button {
                                            class: "btn-discard",
                                            onclick: move |_| handle_discard(m_id, t_id_discard.clone()),
                                            "🗑️ 放弃"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
