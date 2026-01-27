use crate::models::ChatMessage;
use dioxus::prelude::*;

#[component]
pub fn ChatView(messages: Signal<Vec<ChatMessage>>) -> Element {
    rsx! {
        div { class: "chat-scroll",
            for msg in messages.read().iter() {
                div {
                    key: "{msg.id}",
                    class: if msg.is_user { "message msg-user" } else { "message msg-ai" },
                    style: "white-space: pre-wrap;",
                    "{msg.text}"
                }
            }
        }
    }
}
