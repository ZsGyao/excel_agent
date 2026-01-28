use crate::{components::data_table::DataTable, models::ChatMessage};
use dioxus::prelude::*;

#[component]
pub fn ChatView(messages: Signal<Vec<ChatMessage>>) -> Element {
    rsx! {
        div { class: "chat-scroll",
            for msg in messages.read().iter() {
                div {
                    key: "{msg.id}",
                    class: if msg.is_user { "message msg-user" } else { "message msg-ai" },

                    // Show text message
                    div { style: "white-space: pre-wrap;", "{msg.text}" }

                    // If have table render table
                    if let Some(table_data) = &msg.table {
                        DataTable { data: table_data.clone() }
                    }
                }
            }
        }
    }
}
