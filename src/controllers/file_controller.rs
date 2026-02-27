use crate::models::PendingImport;
use crate::services::excel_engine::ExcelEngine;
use crate::store::app_state::AppState;
use dioxus::prelude::*;
use std::collections::HashMap;

pub fn open_file_dialog(mut state: AppState, tx_files: Coroutine<String>) {
    spawn(async move {
        if let Some(path) = rfd::AsyncFileDialog::new()
            .add_filter("Excel", &["xlsx", "xls", "xlsm"])
            .pick_file()
            .await
        {
            let full_path = path.path().to_string_lossy().to_string();
            tx_files.send(full_path.clone());

            match ExcelEngine::get_sheet_names(&full_path) {
                Ok(sheet_names) => state
                    .pending_import
                    .set(Some(PendingImport::new(full_path, sheet_names))),
                Err(e) => println!("读取Excel失败: {}", e),
            }
        }
    });
}

pub fn handle_import_confirm(
    mut state: AppState,
    file_path: String,
    sheet_configs: HashMap<String, usize>,
) {
    spawn(async move {
        match ExcelEngine::parse_file_with_config(&file_path, &sheet_configs) {
            Ok(schema) => {
                state
                    .global_schemas
                    .write()
                    .insert(file_path.clone(), schema);
                let mut files = state.active_files.write();
                if !files.contains(&file_path) {
                    files.push(file_path);
                }
                state.pending_import.set(None);
            }
            Err(e) => {
                println!("精准解析失败: {}", e);
                state.pending_import.set(None);
            }
        }
    });
}

pub fn handle_import_cancel(mut state: AppState) {
    state.pending_import.set(None);
}

pub fn remove_file(mut state: AppState, path: String) {
    state.active_files.write().retain(|f| f != &path);
}

pub fn clear_all_files(mut state: AppState) {
    state.active_files.write().clear();
}
