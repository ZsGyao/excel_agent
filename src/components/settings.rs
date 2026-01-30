use crate::models::{AppConfig, ModelProfile};
use crate::services::config::save_config;
use dioxus::prelude::*;

#[component]
pub fn Settings(
    config: Signal<AppConfig>,
    on_close: EventHandler<()>, // 🔥 新增：用于通知父组件关闭设置窗口
) -> Element {
    // 用于暂存当前正在编辑的配置（深拷贝）
    let mut editing_profile = use_signal(|| ModelProfile::new());

    // 监听 config 变化，自动选中当前激活的 profile
    use_effect(move || {
        let cfg = config.read();
        if let Some(active_id) = &cfg.active_profile_id {
            if let Some(profile) = cfg.profiles.iter().find(|p| &p.id == active_id) {
                editing_profile.set(profile.clone());
            }
        }
    });

    // 保存逻辑 (自动保存)
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

    // 新增 Profile
    let mut add_profile = move || {
        let mut current_config = config.read().clone();
        let new_profile = ModelProfile::new();
        let new_id = new_profile.id.clone();

        current_config.profiles.push(new_profile);
        current_config.active_profile_id = Some(new_id); // 自动选中新建的

        config.set(current_config.clone());
        save_config(&current_config);
    };

    // 删除 Profile
    let mut delete_profile = move |id: String| {
        let mut current_config = config.read().clone();
        if current_config.profiles.len() <= 1 {
            return; // 至少保留一个
        }

        current_config.profiles.retain(|p| p.id != id);

        // 如果删除了当前选中的，就选中第一个
        if current_config.active_profile_id.as_ref() == Some(&id) {
            if let Some(first) = current_config.profiles.first() {
                current_config.active_profile_id = Some(first.id.clone());
            }
        }

        config.set(current_config.clone());
        save_config(&current_config);
    };

    // 准备数据用于渲染
    let profiles = config.read().profiles.clone();
    let active_id = config.read().active_profile_id.clone();
    let profiles_count = profiles.len();

    rsx! {
        div { class: "settings-layout",
            // === 顶部栏 ===
            div { class: "settings-header",
                div { class: "settings-title", "配置中心" }
                // 返回按钮
                div {
                    class: "settings-close-btn",
                    onclick: move |_| on_close.call(()),
                    "返回"
                }
            }

            // === 内容区 (左右分栏) ===
            div { class: "settings-body",

                // --- 左侧：模型列表 ---
                div { class: "settings-sidebar",
                    div { class: "sidebar-label", "可用模型" }

                    // 遍历列表
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
                                        class: if is_active { "model-item active" } else { "model-item" }, // 防止触发切换
                                        // 点击切换
                                        onclick: move |_| {
                                            let mut cfg = config.read().clone();
                                            cfg.active_profile_id = Some(id_for_click.clone());
                                            config.set(cfg.clone());
                                            save_config(&cfg);
                                        },

                                        div { style: "display: flex; justify-content: space-between; align-items: center;",
                                            div { class: "model-name", "{profile.name}" }

                                            // 删除按钮 (仅当多于1个时显示)
                                            if profiles_count > 1 {
                                                div {
                                                    class: "del-btn", // 需要自己在 CSS 加个简单样式或者直接用文字
                                                    style: "color: #999; font-size: 12px; padding: 4px;",
                                                    onclick: move |evt| {
                                                        evt.stop_propagation(); // 防止触发切换 // 防止触发切换
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

                    // 新增按钮
                    div {
                        class: "add-model-btn",
                        onclick: move |_| add_profile(),
                        "+ 新增配置"
                    }
                }

                // --- 右侧：编辑表单 ---
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

                    // 底部说明 (可选)
                    div { style: "margin-top: 30px; font-size: 12px; color: #999; text-align: center;",
                        "配置会自动保存"
                    }
                }
            }
        }
    }
}
