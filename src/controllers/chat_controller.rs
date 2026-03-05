use crate::models::ActionStatus;
use crate::services::ai::call_ai;
use crate::services::python::{create_batch_backups, run_batch_hot_undo, run_python_code};
use crate::store::app_state::AppState;
use crate::utils::file::is_file_locked;
use dioxus::prelude::*;

const MAX_RETRIES: i32 = 3;

pub fn on_confirm(mut state: AppState, msg_id: usize) {
    // 1. 瞬间获取消息状态和环境上下文，立刻释放锁
    let (mut code_opt, user_query, current_files, has_existing_backup) = {
        let mut msgs = state.messages.write();

        let extracted_query = if msg_id > 0 {
            msgs[msg_id - 1].text.clone() // 拿到用户的提问
        } else {
            String::new()
        };

        let msg = &mut msgs[msg_id];

        // 如果已经有代码了，直接标记为运行中
        if msg.pending_code.is_some() {
            msg.status = ActionStatus::Running;
        }

        (
            msg.pending_code.clone(),
            extracted_query, // ✅ 拿到正确的提问传出去！
            state.active_files.read().clone(),
            msg.backup_paths.is_some(),
        )
    };

    spawn(async move {
        // ==========================================
        // 🌟 阶段 A：如果还没有代码，立刻嗅探并呼叫 AI
        // ==========================================
        if code_opt.is_none() {
            // 更新 UI 为加载状态
            if let Some(msg) = state.messages.write().get_mut(msg_id) {
                msg.status = ActionStatus::Running;
            }

            // 🕵️ 1. 极其关键的“最后一毫秒”嗅探！
            let mut is_any_file_opened = false;
            for file_path in &current_files {
                if is_file_locked(file_path) {
                    println!(
                        "🕵️ 警报：检测到表格 [{}] 正在被占用，强制走 xlwings 热更新！",
                        file_path
                    );
                    is_any_file_opened = true;
                    break; // 只要有一个打开了，就走热更新模式
                }
            }

            // 2. 拿出全局最新的脱水 JSON
            let schema_json_str = state.ai_context_json.read().clone();
            let schemas_ref = state.global_schemas.read();

            // 3. 带着环境状态，去向 AI 索要代码
            match call_ai(
                &state.config.read(),
                &user_query,
                &schema_json_str,
                is_any_file_opened,
                &*schemas_ref,
            )
            .await
            {
                Ok(generated_code) => {
                    // 把 AI 写的代码存入消息中，并传递给下一步
                    if let Some(msg) = state.messages.write().get_mut(msg_id) {
                        msg.pending_code = Some(generated_code.clone());
                    }
                    code_opt = Some(generated_code);
                }
                Err(api_error) => {
                    // AI 报错，直接中断并提示用户
                    if let Some(msg) = state.messages.write().get_mut(msg_id) {
                        msg.status = ActionStatus::Error(api_error.clone());
                        msg.text
                            .push_str(&format!("\n\n❌ AI 大脑宕机:\n{}", api_error));
                    }
                    return;
                }
            }
        }

        // ==========================================
        // 🌟 阶段 B：执行 Python 代码 (自愈内循环)
        // ==========================================
        if let Some(code) = code_opt {
            // 备份防灾
            if !has_existing_backup {
                let backups = if !current_files.is_empty() {
                    create_batch_backups(current_files).await
                } else {
                    Vec::new()
                };
                if !backups.is_empty() {
                    state.messages.write()[msg_id].backup_paths = Some(backups);
                }
            }

            // 🌟 提取配置信息（给 Fixer AI 请求使用）
            let (api_key, active_model, base_url) = {
                let cfg = state.config.read();
                (
                    cfg.active_profile().api_key.clone(),
                    cfg.active_profile().model_id.clone(),
                    cfg.active_profile().base_url.clone(),
                )
            };

            // 提取当前的表头架构 JSON
            let schemas_json =
                serde_json::to_string(&*state.global_schemas.read()).unwrap_or_default();

            let mut current_code = code.clone();
            let mut attempt = 0;

            // 🌟 开启自愈内循环
            loop {
                // 运行当前的 Python 代码
                let res = run_python_code(&current_code).await;

                let mut msgs = state.messages.write();
                let Some(msg) = msgs.get_mut(msg_id) else {
                    break;
                };

                match res {
                    Ok(out) => {
                        msg.status = ActionStatus::Success;
                        // 正则剥离结构化结果
                        // (?s) 允许跨行匹配，(.*?) 非贪婪匹配取出中间的所有内容
                        let re = regex::Regex::new(r"(?s)<REPORT>(.*?)</REPORT>").unwrap();
                        let mut extracted_reports = Vec::new();

                        for cap in re.captures_iter(&out) {
                            let content = cap[1].trim();
                            if !content.is_empty() {
                                extracted_reports.push(content.to_string());
                            }
                        }

                        // 如果 AI 按照规范输出了 REPORT 标签，就优雅地展示
                        if !extracted_reports.is_empty() {
                            let final_report = extracted_reports.join("\n\n---\n\n");
                            msg.text
                                .push_str(&format!("\n\n📊 **执行报告:**\n{}", final_report));
                        } else {
                            // 如果 AI 没有输出任何 REPORT（比如只是单纯的修改、清空数据操作）
                            // 我们就不显示多余的文字，保持界面极简
                            // (UI 上的 ✅ 操作已完成 Badge 已经足够说明一切了)
                        }

                        break; // 执行成功，跳出循环
                    }
                    Err(e) => {
                        msg.status = ActionStatus::Error(e.clone());
                        msg.text.push_str(&format!("\n\n❌ 错误:\n{}", e));

                        if attempt >= MAX_RETRIES {
                            msg.text.push_str(&format!(
                                "\n\n🛑 已达到最大重试次数 ({})，停止自动修复。",
                                MAX_RETRIES
                            ));
                            break; // 🛑 达到重试上限，彻底退出
                        }

                        attempt += 1;
                        msg.text.push_str(&format!(
                            "\n\n🔄 自动修复中 (尝试 {}/{})...",
                            attempt, MAX_RETRIES
                        ));
                        msg.status = ActionStatus::Running;

                        // 必须在这里释放读写锁，否则下面的 await 请求会导致死锁！
                        drop(msgs);

                        // 🌟 呼叫 Fixer AI 进行抢救
                        match crate::services::ai::fix_code(
                            &user_query, // 直接复用最顶部提取的用户提问
                            &current_code,
                            &e,
                            &api_key,
                            &active_model,
                            &base_url,
                            &schemas_json,
                        )
                        .await
                        {
                            Ok(fixed_code) => {
                                current_code = fixed_code.clone(); // 覆盖为修复后的代码

                                // 更新 UI 的代码面板，让用户直观看到代码被改了
                                let mut msgs = state.messages.write();
                                if let Some(msg) = msgs.get_mut(msg_id) {
                                    msg.pending_code = Some(fixed_code);
                                }
                                // 循环继续，马上自动执行新的 current_code
                            }
                            Err(ai_err) => {
                                let mut msgs = state.messages.write();
                                if let Some(msg) = msgs.get_mut(msg_id) {
                                    msg.status = ActionStatus::Error(ai_err.clone());
                                    msg.text
                                        .push_str(&format!("\n\n🛑 修复代码生成失败: {}", ai_err));
                                }
                                break; // 🛑 LLM 接口调用失败，直接退出循环
                            }
                        }
                    }
                }
            }
        }
    });

    state.is_loading.set(false);
}

pub fn on_cancel(mut state: AppState, id: usize) {
    if let Some(msg) = state.messages.write().get_mut(id) {
        msg.status = ActionStatus::Cancelled;
        msg.pending_code = None;
        state.retry_count.set(0);
    }
}

pub fn on_undo(mut state: AppState, target_msg_id: usize) {
    let backup_pairs = state
        .messages
        .read()
        .get(target_msg_id)
        .and_then(|m| m.backup_paths.clone());
    if let Some(pairs) = backup_pairs {
        spawn(async move {
            let res = run_batch_hot_undo(pairs).await;
            let mut msgs = state.messages.write();
            for i in target_msg_id..msgs.len() {
                if let Some(m) = msgs.get_mut(i) {
                    if matches!(m.status, ActionStatus::Success | ActionStatus::Running) {
                        m.status = ActionStatus::Undone;
                        if i == target_msg_id {
                            match res {
                                Ok(ref log) => m.text.push_str(&format!("\n\n{}", log)),
                                Err(ref e) => m.text.push_str(&format!("\n❌ 撤销出错: {}", e)),
                            }
                        } else {
                            m.text.push_str("\n(因回溯已失效)");
                        }
                    }
                }
            }
        });
    }
}
