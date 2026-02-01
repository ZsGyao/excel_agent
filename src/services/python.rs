use pyo3::prelude::*;
use std::env;
use std::fs;
use std::path::Path;
use std::sync::Once;

static INIT: Once = Once::new();

/// 初始化嵌入式 Python 环境
/// 必须在主线程启动时调用一次
pub fn init_python_env() {
    INIT.call_once(|| {
        // 配置本地便携式 Python 环境
        // 获取当前运行目录
        let current_dir = env::current_dir().unwrap_or_else(|_| Path::new(".").to_path_buf());
        let py_env_path = current_dir.join("py_env");

        // 检查 py_env 是否存在，如果不存在打印警告（方便调试）
        if !py_env_path.exists() {
            println!(
                "⚠️ 警告：未找到本地 py_env 目录，将尝试使用系统 Python。路径: {:?}",
                py_env_path
            );
        } else {
            println!("✅ 检测到本地 Python 环境: {:?}", py_env_path);

            // 设置标准库压缩包 (根据你的实际文件名修改，比如 python311.zip)
            let std_lib = py_env_path.join("python311.zip");
            // 设置第三方库目录
            let site_packages = py_env_path.join("Lib").join("site-packages");
            // 设置 DLL 目录
            let dlls = py_env_path.join("DLLs");

            // 拼接 PYTHONPATH (Windows 使用分号 ; 分隔)
            let new_python_path = format!(
                "{};{};{}",
                std_lib.display(),
                site_packages.display(),
                dlls.display()
            );

            // 强制设置环境变量
            // 告诉 Python 解释器：家就在这里，别去系统里找
            env::set_var("PYTHONHOME", &py_env_path);
            env::set_var("PYTHONPATH", &new_python_path);

            // 可选：把 py_env 也加到系统 PATH 里，防止找不到 python3.dll
            if let Ok(path) = env::var("PATH") {
                let new_path = format!("{};{}", py_env_path.display(), path);
                env::set_var("PATH", new_path);
            }
        }

        // 初始化 PyO3 解释器
        // 此时它会读取上面设置的 PYTHONHOME
        pyo3::prepare_freethreaded_python();
        println!("🐍 Python 解释器初始化完成");
    });
}

/// 异步运行 Python 代码 (xlwings 热更新的核心)
pub async fn run_python_code(code: &str) -> Result<String, String> {
    let code = code.to_string();

    // 放入 blocking 线程池，防止卡死 UI
    let result = tokio::task::spawn_blocking(move || {
        Python::with_gil(|py| {
            let sys = py.import("sys").map_err(|e| e.to_string())?;
            let io = py.import("io").map_err(|e| e.to_string())?;
            let stdout_capture = io.call_method0("StringIO").map_err(|e| e.to_string())?;

            // 劫持标准输出
            sys.setattr("stdout", stdout_capture)
                .map_err(|e| e.to_string())?;
            sys.setattr("stderr", stdout_capture)
                .map_err(|e| e.to_string())?;

            // 执行代码
            let run_result = py.run(&code, None, None);

            // 获取输出
            let output = stdout_capture
                .call_method0("getvalue")
                .map_err(|e| e.to_string())?
                .extract::<String>()
                .map_err(|e| e.to_string())?;

            match run_result {
                Ok(_) => Ok(output),
                Err(e) => {
                    let traceback =
                        format!("Python Runtime Error:\n{}\n\nOutput log:\n{}", e, output);
                    Err(traceback)
                }
            }
        })
    })
    .await;

    match result {
        Ok(python_result) => python_result.map_err(|e| e.to_string()),
        Err(e) => Err(format!("System Task Error: {}", e)),
    }
}

/// 快速读取 Excel 表头信息 (用于 AI 上下文)
pub async fn get_excel_info(file_path: &str) -> String {
    if !Path::new(file_path).exists() {
        return "文件不存在".to_string();
    }

    let file_path = file_path.to_string();

    let result = tokio::task::spawn_blocking(move || {
        Python::with_gil(|py| {
            // 仅读取 columns，nrows=0 极速模式
            let code = format!(
                r#"
import pandas as pd
try:
    df = pd.read_excel(r"{}", nrows=0)
    print(f"Columns: {{list(df.columns)}}")
except Exception as e:
    print(f"Read Info Error: {{e}}")
"#,
                file_path
            );

            let sys = py.import("sys").ok()?;
            let io = py.import("io").ok()?;
            let stdout_capture = io.call_method0("StringIO").ok()?;
            sys.setattr("stdout", stdout_capture).ok()?;

            let _ = py.run(&code, None, None);

            let output = stdout_capture
                .call_method0("getvalue")
                .ok()?
                .extract::<String>()
                .ok()?;
            Some(output.trim().to_string())
        })
    })
    .await;

    match result {
        Ok(Some(info)) => info,
        _ => "无法读取文件信息".to_string(),
    }
}

/// 备份文件 (撤销功能依赖)
pub fn backup_file(file_path: &str) -> Option<String> {
    let path = Path::new(file_path);
    if !path.exists() {
        return None;
    }

    let backup_path = format!("{}.bak", file_path);
    match fs::copy(file_path, &backup_path) {
        Ok(_) => Some(backup_path),
        Err(e) => {
            println!("备份失败: {}", e);
            None
        }
    }
}

/// 恢复文件
pub fn restore_file(original_path: &str, backup_path: &str) -> Result<(), String> {
    match fs::copy(backup_path, original_path) {
        Ok(_) => Ok(()),
        Err(e) => Err(format!("恢复失败: {}", e)),
    }
}
