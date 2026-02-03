use pyo3::prelude::*;
use std::env;
use std::fs;
use std::path::Path;
use std::sync::Once;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

static INIT: Once = Once::new();

/// 初始化 Python 环境
///
/// 确保 py_env 存在并设置环境变量，以便 xlwings 和 pandas 能正常运行。
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

/// 启动时清理备份文件夹
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

/// 异步运行 Python 代码
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

/// 🔥 多文件上下文生成
///
/// 遍历传入的所有文件路径，依次读取前5行，并拼接成一个大的 Markdown 上下文。
/// 这样 AI 就能知道 "File A 有这些列，File B 有那些列"。
pub async fn get_multi_file_summary(file_paths: Vec<String>) -> String {
    if file_paths.is_empty() {
        return String::new();
    }

    let result = tokio::task::spawn_blocking(move || {
        Python::with_gil(|py| -> String {
            // 动态生成 Python 代码，循环读取列表
            let paths_repr = format!("{:?}", file_paths); // 转为 Python List 字符串

            let code = format!(
                r#"
import pandas as pd
import os

file_paths = {}
final_report = ""

for path in file_paths:
    if not os.path.exists(path):
        continue
    
    filename = os.path.basename(path)
    final_report += f"\n=== File: {{filename}} ===\nPath: {{path}}\n"
    
    try:
        df = pd.read_excel(path, nrows=5)
        info = "Columns & Types:\n"
        for col in df.columns:
            info += f"- {{col}}: {{df[col].dtype}}\n"
        info += "\nPreview:\n"
        try:
            info += df.to_markdown(index=False)
        except ImportError:
            info += df.to_string(index=False)
        except Exception:
            info += "[Preview Error]"
        
        final_report += info + "\n" + "-"*30 + "\n"
    except Exception as e:
        final_report += f"Error reading file: {{e}}\n"

print(final_report)
"#,
                paths_repr
            );

            let sys = match py.import("sys") {
                Ok(v) => v,
                Err(_) => return "Sys import failed".into(),
            };
            let io = match py.import("io") {
                Ok(v) => v,
                Err(_) => return "IO import failed".into(),
            };
            let stdout = match io.call_method0("StringIO") {
                Ok(v) => v,
                Err(_) => return "StringIO failed".into(),
            };
            if sys.setattr("stdout", stdout).is_err() {
                return "Set stdout failed".into();
            }

            let _ = py.run(&code, None, None);

            if let Ok(out) = stdout.call_method0("getvalue") {
                if let Ok(s) = out.extract::<String>() {
                    return s;
                }
            }
            "Read Output failed".into()
        })
    })
    .await;

    // 如果 Python 内部失败返回 None，或者线程失败，都返回默认提示
    result.unwrap_or_else(|_| "系统错误".to_string())
}

/// 批量热备份
///
/// 针对传入的所有文件，依次调用 Excel SaveCopyAs。
/// 返回：Vec<(原路径, 备份路径)>
pub async fn create_batch_backups(target_paths: Vec<String>) -> Vec<(String, String)> {
    let mut backups = Vec::new();
    let backup_dir = env::current_dir().unwrap_or_default().join("backups");
    if !backup_dir.exists() {
        let _ = fs::create_dir_all(&backup_dir);
    }

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();

    for path in target_paths {
        let file_name = Path::new(&path)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();
        // 备份名为：原名.时间戳.bak，防止冲突
        let backup_filename = format!("{}.{}.bak", file_name, timestamp);
        let backup_path = backup_dir
            .join(&backup_filename)
            .to_string_lossy()
            .to_string();

        let code = format!(
            r#"
import xlwings as xw
import os
import shutil

target = r"{}"
backup = r"{}"

try:
    wb = None
    try:
        wb = xw.books[os.path.basename(target)]
    except:
        for app in xw.apps:
            for book in app.books:
                if book.fullname.lower() == target.lower():
                    wb = book; break
            if wb: break
    
    if wb:
        wb.api.SaveCopyAs(backup)
    else:
        # 如果文件没打开，直接物理复制
        shutil.copy2(target, backup)
except:
    pass
"#,
            path, backup_path
        );

        // 我们这里串行执行备份，虽然稍慢但逻辑简单安全
        if let Ok(_) = run_python_code(&code).await {
            // 只要没报错，就认为备份成功（即便可能是物理复制）
            if Path::new(&backup_path).exists() {
                backups.push((path, backup_path));
            }
        }
    }
    backups
}

/// 🔥 批量热撤销
///
/// 接收一组 (原路径, 备份路径) 的列表，依次恢复。
pub async fn run_batch_hot_undo(restore_pairs: Vec<(String, String)>) -> Result<String, String> {
    // 构造一个 Python 列表传入，在 Python 端循环处理，减少进程交互开销
    let pairs_repr = format!("{:?}", restore_pairs);

    let code = format!(
        r#"
import xlwings as xw
import os

# list of (target, backup)
pairs = {}

log = []

for target_file, backup_file in pairs:
    try:
        # 1. 找目标 Workbook
        wb_target = None
        try:
            wb_target = xw.books[os.path.basename(target_file)]
        except:
            for app in xw.apps:
                for book in app.books:
                    if book.fullname.lower() == target_file.lower():
                        wb_target = book; break
                if wb_target: break
        
        if not wb_target:
            log.append(f"⚠️ 跳过 {{os.path.basename(target_file)}}: 未打开")
            continue

        # 2. 打开备份并恢复
        app = wb_target.app
        wb_backup = app.books.open(backup_file)
        
        # 恢复当前激活 Sheet (简化版，生产环境可能需要恢复所有 Sheet)
        target_sheet = wb_target.sheets.active
        sheet_name = target_sheet.name
        
        found = False
        for s in wb_backup.sheets:
            if s.name == sheet_name:
                target_sheet.clear() 
                s.used_range.copy(target_sheet.range('A1'))
                found = True
                break
        
        wb_backup.close()
        
        if found:
            log.append(f"✅ 已恢复 {{os.path.basename(target_file)}}")
        else:
            log.append(f"⚠️ {{os.path.basename(target_file)}} 恢复失败: Sheet不匹配")

    except Exception as e:
        log.append(f"❌ {{os.path.basename(target_file)}} 错误: {{e}}")

print("\n".join(log))
"#,
        pairs_repr
    );

    run_python_code(&code).await
}

/// 物理恢复函数（用于降级）
pub fn restore_file_physical(original: &str, backup: &str) -> Result<(), std::io::Error> {
    fs::copy(backup, original)?;
    Ok(())
}
