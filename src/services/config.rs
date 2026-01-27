use crate::models::AppConfig;
use directories::ProjectDirs;
use std::fs;
use std::path::PathBuf;

const APP_QUALIFIER: &str = "com";
const APP_ORG: &str = "excelagent";
const APP_NAME: &str = "excel_agent";

/// 获取配置文件路径: C:\Users\xxx\AppData\Local\excelagent\excel_agent\config.json
fn get_config_path() -> PathBuf {
    if let Some(proj_dirs) = ProjectDirs::from(APP_QUALIFIER, APP_ORG, APP_NAME) {
        let config_dir = proj_dirs.data_local_dir();
        // 确保目录存在
        if !config_dir.exists() {
            let _ = fs::create_dir_all(config_dir);
        }
        return config_dir.join("config.json");
    }
    // 回退方案：当前目录
    PathBuf::from("config.json")
}

/// 加载配置
pub fn load_config() -> AppConfig {
    let path = get_config_path();
    if path.exists() {
        if let Ok(content) = fs::read_to_string(path) {
            if let Ok(config) = serde_json::from_str::<AppConfig>(&content) {
                return config;
            }
        }
    }
    // 如果没有文件或解析失败，返回默认值
    AppConfig::default()
}

/// 保存配置
pub fn save_config(config: &AppConfig) {
    let path = get_config_path();
    if let Ok(json) = serde_json::to_string_pretty(config) {
        let _ = fs::write(path, json);
    }
}
