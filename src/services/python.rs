use pyo3::prelude::*;
use std::env;
use std::fs;
use std::path::Path;
use std::sync::Once;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

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

// 启动时清理备份文件夹
pub fn cleanup_backups() {
    let backup_dir = Path::new("backups");
    // 如果存在，先删除整个目录（清空旧文件）
    if backup_dir.exists() {
        let _ = fs::remove_dir_all(backup_dir);
    }
    // 重新创建空目录
    if let Err(e) = fs::create_dir_all(backup_dir) {
        println!("⚠️ 无法创建备份目录: {}", e);
    } else {
        println!("✅ 备份目录已重置: backups/");
    }
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

/// 获取 Excel 数据概览 (表头 + 前5行)
pub async fn get_excel_summary(file_path: &str) -> String {
    if !Path::new(file_path).exists() {
        return "文件不存在".to_string();
    }
    let file_path = file_path.to_string();

    let result = tokio::task::spawn_blocking(move || {
        Python::with_gil(|py| -> String {
            // 使用 pandas 快速读取前 5 行，并转为 markdown 格式字符串
            let code = format!(
                r#"
import pandas as pd
try:
    d# 只读取前5行，避免大文件卡死
    df = pd.read_excel(r"{}", nrows=5)
    
    info = "Columns & Types:\n"
    for col in df.columns:
        info += f"- {{col}}: {{df[col].dtype}}\n"
    
    info += "\nData Preview (First 5 rows):\n"
    
    # 🔥 核心容错逻辑 🔥
    try:
        # 优先尝试 Markdown (需要 tabulate 库)
        info += df.to_markdown(index=False)
    except ImportError:
        # 如果没装 tabulate，降级使用默认 string 格式
        info += df.to_string(index=False)
    except Exception as e:
        info += f"[Preview Error: {{e}}]"

    print(info)
except Exception as e:
    print(f"无法读取数据预览: {{e}}")
"#,
                file_path
            );

            let sys = py.import("sys").unwrap();
            let io = py.import("io").unwrap();
            let stdout = io.call_method0("StringIO").unwrap();
            sys.setattr("stdout", stdout).unwrap();

            let _ = py.run(&code, None, None);

            if let Ok(out) = stdout.call_method0("getvalue") {
                if let Ok(s) = out.extract::<String>() {
                    return s;
                }
            }
            "读取失败".to_string()
        })
    })
    .await;

    result.unwrap_or_else(|_| "系统错误".to_string())
}

// 热备份 (SaveCopyAs)
pub async fn create_live_backup(target_path: &str) -> Result<String, String> {
    // 1. 确保目录存在 (虽然启动时创建了，但防一手被误删)
    let backup_dir = env::current_dir().unwrap_or_default().join("backups");
    if !backup_dir.exists() {
        let _ = fs::create_dir_all(&backup_dir);
    }

    // 1. 生成唯一时间戳
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let file_name = Path::new(target_path)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy();

    // 路径：<AppDir>/backups/large_test_data.xlsx.17023123.bak
    let backup_path = backup_dir.join(format!("{}.{}.bak", file_name, timestamp));
    // 转为绝对路径字符串传给 Python
    let backup_path_str = backup_path.to_string_lossy().to_string();

    let code = format!(
        r#"
import xlwings as xw
import os
import shutil

target_file = r"{}"
backup_file = r"{}"

try:
    # 尝试连接活跃 Workbook
    wb = None
    try:
        wb = xw.books[os.path.basename(target_file)]
    except:
        for app in xw.apps:
            for book in app.books:
                if book.fullname.lower() == target_file.lower():
                    wb = book; break
            if wb: break
    
    if wb:
        # 保存内存快照
        wb.api.SaveCopyAs(backup_file)
        print("Live Backup Created")
    else:
        # 降级：物理复制
        shutil.copy2(target_file, backup_file)
        print("Static Backup Created")

except Exception as e:
    print(f"Backup Error: {{e}}")
    raise e
"#,
        target_path, backup_path_str
    );

    match run_python_code(&code).await {
        Ok(_) => Ok(backup_path_str), // 返回这个唯一的路径给 main.rs 存起来
        Err(e) => Err(e),
    }
}

// 热撤销逻辑
// 如果文件被锁，使用 xlwings 打开备份文件，把内容复制回当前文件
pub async fn run_hot_undo(target_path: &str, backup_path: &str) -> Result<String, String> {
    let code = format!(
        r#"
import xlwings as xw
import os

target_file = r"{}"
backup_file = r"{}"

try:
    # 1. 连接到当前打开的目标文件
    wb_target = None
    target_name = os.path.basename(target_file).lower()
    try:
        wb_target = xw.books[os.path.basename(target_file)]
    except:
        for app in xw.apps:
            for book in app.books:
                if book.fullname.lower() == target_file.lower():
                    wb_target = book; break
            if wb_target: break
            
    if not wb_target:
        raise Exception("目标文件未打开，无法进行热撤销")

    # 2. 后台打开备份文件
    # 使用同一个 app 打开，方便 sheet 复制
    app = wb_target.app
    wb_backup = app.books.open(backup_file)
    
    # 3. 恢复当前激活的 Sheet
    target_sheet = wb_target.sheets.active
    sheet_name = target_sheet.name
    
    # 在备份里找同名 Sheet
    sheet_names = [s.name for s in wb_backup.sheets]
    if sheet_name in sheet_names:
        src_sheet = wb_backup.sheets[sheet_name]
        
        # 清空目标并复制
        target_sheet.clear() 
        src_sheet.used_range.copy(target_sheet.range('A1'))
        
        print(f"✨ 成功！已从备份恢复 Sheet: {{sheet_name}}")
    else:
        print(f"⚠️ 警告：备份文件中找不到 Sheet: {{sheet_name}}，无法恢复")

    # 4. 关闭备份文件 (不保存)
    wb_backup.close()

except Exception as e:
    print(f"❌ 热撤销失败: {{e}}")
    raise e
"#,
        target_path, backup_path
    );

    run_python_code(&code).await
}

/// 恢复文件
pub fn restore_file(original_path: &str, backup_path: &str) -> Result<(), String> {
    match fs::copy(backup_path, original_path) {
        Ok(_) => Ok(()),
        Err(e) => Err(format!("恢复失败: {}", e)),
    }
}
