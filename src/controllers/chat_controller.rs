use crate::models::ActionStatus;
use crate::services::ai;
use crate::services::python::{create_batch_backups, run_batch_hot_undo, run_python_code};
use crate::store::app_state::AppState;
use crate::utils::file::is_file_locked; // 🌟 引入我们刚写的嗅探器
use dioxus::prelude::*; // 🌟 引入 AI 服务

const MAX_RETRIES: i32 = 3;

pub fn on_confirm(mut state: AppState, msg_id: usize) {
    // 1. 瞬间获取消息状态和环境上下文，立刻释放锁
    let (mut code_opt, user_query, current_files, has_existing_backup) = {
        let mut msgs = state.messages.write();
        let user_query = if msg_id > 0 {
            msgs[msg_id - 1].text.clone()
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
            msg.text.clone(), // 用户的自然语言提问
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

            // 3. 带着环境状态，去向 AI 索要代码
            match ai::call_ai(
                &state.config.read(),
                &user_query,
                &schema_json_str,
                is_any_file_opened,
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
        // 🌟 阶段 B：执行 Python 代码 (原有逻辑保留)
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

            // 唤起 Python 引擎
            let res = run_python_code(&code).await;

            // 处理结果与自动重试逻辑
            let mut msgs = state.messages.write();
            if let Some(msg) = msgs.get_mut(msg_id) {
                match res {
                    Ok(out) => {
                        msg.status = ActionStatus::Success;
                        msg.text.push_str(&format!("\n\n✨ 结果:\n{}", out));
                        state.retry_count.set(0);
                    }
                    Err(e) => {
                        msg.status = ActionStatus::Error(e.clone());
                        msg.text.push_str(&format!("\n\n❌ 错误:\n{}", e));
                        let current_retries = *state.retry_count.read();
                        if current_retries < MAX_RETRIES {
                            state.retry_count.set(current_retries + 1);
                            msg.text.push_str(&format!(
                                "\n\n🔄 自动修复中 (尝试 {}/{})...",
                                current_retries + 1,
                                MAX_RETRIES
                            ));
                            state.error_fix_signal.set(Some(e));
                            // 这里如果要做真正的自动修复，还可以再调一次 AI。目前先保持你的重试循环架构。
                        } else {
                            msg.text.push_str(&format!(
                                "\n\n🛑 已达到最大重试次数 ({})，停止自动修复。",
                                MAX_RETRIES
                            ));
                            state.retry_count.set(0);
                        }
                    }
                }
            }
        }
    });
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
