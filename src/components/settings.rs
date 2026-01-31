use crate::models::{AppConfig, ModelProfile};
use crate::services::config::save_config;
use dioxus::prelude::*;
use std::time::Duration;

#[component]
pub fn Settings(
    config: Signal<AppConfig>,
    on_close: EventHandler<()>, // 这里的 on_close 逻辑已经在 main.rs 里被我们改造成带延迟的了
) -> Element {
    let mut editing_profile = use_signal(|| ModelProfile::new());
    let mut anim_ready = use_signal(|| false);

    use_effect(move || {
        let cfg = config.read();
        if let Some(active_id) = &cfg.active_profile_id {
            if let Some(profile) = cfg.profiles.iter().find(|p| &p.id == active_id) {
                editing_profile.set(profile.clone());
            }
        }
        // 设置界面打开时的淡入延迟
        spawn(async move {
            tokio::time::sleep(Duration::from_millis(200)).await;
            anim_ready.set(true);
        });
    });

    let mut save_changes = move || {
        let mut current_config = config.read().clone();
        let edited = editing_profile.read().clone();
        if let Some(idx) = current_config
            .profiles
            .iter()
            .position(|p| p.id == edited.id)
        {
            current_config.profiles[idx] = edited;
            config.set(current_config.clone());
            save_config(&current_config);
        }
    };

    let mut add_profile = move || {
        let mut current_config = config.read().clone();
        let new_profile = ModelProfile::new();
        let new_id = new_profile.id.clone();
        current_config.profiles.push(new_profile);
        current_config.active_profile_id = Some(new_id);
        config.set(current_config.clone());
        save_config(&current_config);
    };

    let mut delete_profile = move |id: String| {
        let mut current_config = config.read().clone();
        if current_config.profiles.len() <= 1 {
            return;
        }
        current_config.profiles.retain(|p| p.id != id);
        if current_config.active_profile_id.as_ref() == Some(&id) {
            if let Some(first) = current_config.profiles.first() {
                current_config.active_profile_id = Some(first.id.clone());
            }
        }
        config.set(current_config.clone());
        save_config(&current_config);
    };

    let profiles = config.read().profiles.clone();
    let active_id = config.read().active_profile_id.clone();
    let profiles_count = profiles.len();
    let opacity_style = if anim_ready() {
        "opacity: 1;"
    } else {
        "opacity: 0;"
    };

    rsx! {
        div {
            class: "settings-panel",
            style: "{opacity_style} transition: opacity 0.2s ease;",

            div { class: "settings-layout",
                div { class: "settings-header",
                    div { class: "settings-title", "配置中心" }
                    div {
                        class: "settings-close-btn",
                        // 点击关闭时调用 main.rs 传进来的 handle (里面已经有了隐藏逻辑)
                        onclick: move |_| on_close.call(()),
                        "返回聊天"
                    }
                }

                div { class: "settings-body",
                    div { class: "settings-sidebar",
                        div { class: "sidebar-label", "可用模型" }
                        {
                            profiles
                                .into_iter()
                                .map(|profile| {
                                    let p_id = profile.id.clone();
                                    let id_for_click = profile.id.clone();
                                    let id_for_del = profile.id.clone();
                                    let is_active = Some(&p_id) == active_id.as_ref();
                                    rsx! {
                                        div {
                                            key: "{p_id}",
                                            class: if is_active { "model-item active" } else { "model-item" },
                                            onclick: move |_| {
                                                let mut cfg = config.read().clone();
                                                cfg.active_profile_id = Some(id_for_click.clone());
                                                config.set(cfg.clone());
                                                save_config(&cfg);
                                            },
                                            div { style: "display: flex; justify-content: space-between; align-items: center;",
                                                div { class: "model-name", "{profile.name}" }
                                                if profiles_count > 1 {
                                                    div {
                                                        class: "del-btn",
                                                        style: "color: #999; font-size: 12px; padding: 4px;",
                                                        onclick: move |evt| {
                                                            evt.stop_propagation();
                                                            delete_profile(id_for_del.clone());
                                                        },
                                                        "✕"
                                                    }
                                                }
                                            }
                                            div { class: "model-desc", "{profile.model_id}" }
                                        }
                                    }
                                })
                        }
                        div {
                            class: "add-model-btn",
                            onclick: move |_| add_profile(),
                            "+ 新增配置"
                        }
                    }

                    div { class: "settings-content",
                        div { class: "form-header", "编辑详情" }
                        div { class: "form-group",
                            label { "配置名称 (别名)" }
                            input {
                                class: "comic-input",
                                value: "{editing_profile.read().name}",
                                oninput: move |evt| {
                                    editing_profile.write().name = evt.value();
                                    save_changes();
                                },
                            }
                        }
                        div { class: "form-group",
                            label { "API Base URL" }
                            input {
                                class: "comic-input",
                                value: "{editing_profile.read().base_url}",
                                oninput: move |evt| {
                                    editing_profile.write().base_url = evt.value();
                                    save_changes();
                                },
                                placeholder: "https://api.moonshot.cn/v1",
                            }
                        }
                        div { class: "form-group",
                            label { "Model ID (模型名)" }
                            input {
                                class: "comic-input",
                                value: "{editing_profile.read().model_id}",
                                oninput: move |evt| {
                                    editing_profile.write().model_id = evt.value();
                                    save_changes();
                                },
                                placeholder: "moonshot-v1-8k",
                            }
                        }
                        div { class: "form-group",
                            label { "API Key" }
                            input {
                                class: "comic-input",
                                r#type: "password",
                                value: "{editing_profile.read().api_key}",
                                oninput: move |evt| {
                                    editing_profile.write().api_key = evt.value();
                                    save_changes();
                                },
                                placeholder: "sk-...",
                            }
                        }
                        div { style: "margin-top: 30px; font-size: 12px; color: #999; text-align: center;",
                            "配置会自动保存"
                        }
                    }
                }
            }
        }
    }
}
