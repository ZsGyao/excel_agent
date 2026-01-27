use crate::models::{AppConfig, ModelProfile};
use crate::services::config::save_config;
use dioxus::prelude::*;

#[component]
pub fn Settings(config: Signal<AppConfig>) -> Element {
    let mut editing_profile = use_signal(|| ModelProfile::new());

    use_effect(move || {
        let cfg = config.read();
        if let Some(active_id) = &cfg.active_profile_id {
            if let Some(profile) = cfg.profiles.iter().find(|p| &p.id == active_id) {
                editing_profile.set(profile.clone());
            }
        }
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

    // 准备数据
    let profiles = config.read().profiles.clone();
    let active_id = config.read().active_profile_id.clone();
    let profiles_count = profiles.len();

    rsx! {
        div { class: "settings-panel", style: "display: flex; gap: 20px; height: 100%;",

            // 左侧：列表区
            div { style: "flex: 1; border-right: 1px solid #eee; padding-right: 20px;",
                h3 { "模型列表" }
                div { class: "profile-list", style: "display: flex; flex-direction: column; gap: 10px;",

                    // ✅ 修复点：改用 .into_iter().map()
                    // 这样我们就可以在闭包里面写 let 语句了
                    {profiles.into_iter().map(|profile| {
                        // 在这里提前克隆 ID，避免所有权冲突
                        let id_for_click = profile.id.clone();
                        let id_for_delete = profile.id.clone();
                        let p_id = profile.id.clone();
                        let is_active = Some(&p_id) == active_id.as_ref();

                        rsx! {
                            div {
                                key: "{p_id}",
                                class: if is_active { "profile-item active" } else { "profile-item" },
                                style: "padding: 10px; border-radius: 8px; cursor: pointer; border: 1px solid #ddd; background: white;",

                                onclick: move |_| {
                                    let mut cfg = config.read().clone();
                                    cfg.active_profile_id = Some(id_for_click.clone());
                                    config.set(cfg.clone());
                                    save_config(&cfg);
                                },

                                div { style: "display: flex; justify-content: space-between; align-items: center;",
                                    span { style: "font-weight: bold;", "{profile.name}" }

                                    if profiles_count > 1 {
                                        button {
                                            style: "background: none; border: none; cursor: pointer; color: #ff4444;",
                                            onclick: move |evt| {
                                                evt.stop_propagation();
                                                delete_profile(id_for_delete.clone());
                                            },
                                            "🗑️"
                                        }
                                    }
                                }
                                div { style: "font-size: 10px; color: #888;", "{profile.model_id}" }
                            }
                        }
                    })}
                }
                button {
                    style: "margin-top: 15px; width: 100%; padding: 8px; background: #eee; border: none; border-radius: 6px; cursor: pointer;",
                    onclick: move |_| add_profile(),
                    "+ 新增配置"
                }
            }

            // 右侧：编辑表单
            div { style: "flex: 2;",
                h3 { "编辑详情" }
                div { class: "settings-group",
                    label { "配置名称 (别名)" }
                    input {
                        value: "{editing_profile.read().name}",
                        oninput: move |evt| { editing_profile.write().name = evt.value(); save_changes(); }
                    }
                }
                div { class: "settings-group",
                    label { "API Base URL" }
                    input {
                        value: "{editing_profile.read().base_url}",
                        oninput: move |evt| { editing_profile.write().base_url = evt.value(); save_changes(); }
                    }
                    p { style: "color: #999; font-size: 11px;", "示例: https://api.moonshot.cn/v1" }
                }
                div { class: "settings-group",
                    label { "Model ID (模型名)" }
                    input {
                        value: "{editing_profile.read().model_id}",
                        oninput: move |evt| { editing_profile.write().model_id = evt.value(); save_changes(); }
                    }
                }
                div { class: "settings-group",
                    label { "API Key" }
                    input {
                        type: "password",
                        value: "{editing_profile.read().api_key}",
                        oninput: move |evt| { editing_profile.write().api_key = evt.value(); save_changes(); },
                        placeholder: "sk-..."
                    }
                }
            }
        }
    }
}
