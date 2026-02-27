use crate::models::ActionStatus;
use crate::services::python::{create_batch_backups, run_batch_hot_undo, run_python_code};
use crate::store::app_state::AppState;
use dioxus::prelude::*;

const MAX_RETRIES: i32 = 3;

pub fn on_confirm(mut state: AppState, msg_id: usize) {
    let (code_opt, current_files, has_existing_backup) = {
        let mut msgs = state.messages.write();
        let msg = &mut msgs[msg_id];
        let code = msg.pending_code.clone();
        if code.is_some() {
            msg.status = ActionStatus::Running;
        }
        (
            code,
            state.active_files.read().clone(),
            msg.backup_paths.is_some(),
        )
    };

    if let Some(code) = code_opt {
        spawn(async move {
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

            let res = run_python_code(&code).await;
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
        });
    }
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
