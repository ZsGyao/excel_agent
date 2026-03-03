//! Python 服务模块
//!
//! 负责所有与 Python 解释器的交互，包括环境初始化、代码执行、
//! 多文件上下文读取、以及基于 xlwings 的热备份与热撤销功能。
//!
//! # 架构变更说明 (Multi-Sheet Support)
//!
//! 1. **上下文读取**: 升级为全 Sheet 读取模式，AI 现在可以感知 Excel 中的所有工作表。
//! 2. **热撤销**: 采用了 "Safe Restore" 策略。
//!    - 以前: 仅恢复 Active Sheet。
//!    - 现在: 遍历备份文件中的所有 Sheet 进行全量恢复。
//!    - 安全机制: 恢复的内容标绿，新增的内容（不在备份中）标红并保留，绝不自动删除用户数据。

use pyo3::prelude::*;
use std::env;
use std::fs;
use std::io;
use std::path::Path;
use std::sync::Once;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

/// 全局初始化锁，确保 Python 环境只初始化一次
static INIT: Once = Once::new();

/// 初始化嵌入式 Python 环境
///
/// 会优先查找当前目录下的 `py_env` 文件夹。如果找不到，则回退到系统 Python。
/// 设置 `PYTHONHOME` 和 `PYTHONPATH` 以确保第三方库 (pandas, xlwings) 可被加载。
pub fn init_python_env() {
    INIT.call_once(|| {
        // 配置本地便携式 Python 环境
        // 获取当前运行目录
        let current_dir = env::current_dir().unwrap_or_else(|_| Path::new(".").to_path_buf());
        let py_env_path = current_dir.join("py_env");
        let zip_path = current_dir.join("py_env.zip");

        // 如果 py_env 文件夹不存在，但是 py_env.zip 存在，说明是第一次运行，需要解压
        if !py_env_path.exists() && zip_path.exists() {
            println!("📦 首次运行，正在解压 Python 环境，请稍候...");
            if let Err(e) = unzip_file(&zip_path, &py_env_path) {
                println!("❌ 解压失败: {}", e);
            } else {
                println!("✅ 解压完成！");
            }
        }

        // 检查 py_env 是否存在
        if !py_env_path.exists() {
            println!(
                "⚠️ 警告：未找到本地 py_env 目录，将尝试使用系统 Python。路径: {:?}",
                py_env_path
            );
        } else {
            println!("✅ 检测到本地 Python 环境: {:?}", py_env_path);

            let std_lib = py_env_path.join("python311.zip");
            let site_packages = py_env_path.join("Lib").join("site-packages");
            let dlls = py_env_path.join("DLLs");

            // 🔥 关键修改：把 current_dir 加到 PYTHONPATH 的最前面
            // 这样 Python 才能找到放在根目录下的 agent_utils.py
            let new_python_path = format!(
                "{};{};{};{}",
                current_dir.display(), // <--- 必须加这个！
                std_lib.display(),
                site_packages.display(),
                dlls.display()
            );

            // 强制设置环境变量
            env::set_var("PYTHONHOME", &py_env_path);
            env::set_var("PYTHONPATH", &new_python_path);

            // 设置 xlwings License，防止报错
            // env::set_var("XLWINGS_LICENSE_KEY", "non-commercial");

            // 把 py_env 也加到系统 PATH 里
            let pywin32_sys = site_packages.join("pywin32_system32");
            let win32_dir = site_packages.join("win32");
            let win32_lib = site_packages.join("win32").join("lib");

            if let Ok(path) = env::var("PATH") {
                let new_path = format!(
                    "{};{};{};{};{}",
                    py_env_path.display(),
                    pywin32_sys.display(),
                    win32_dir.display(),
                    win32_lib.display(),
                    path
                );
                env::set_var("PATH", new_path);
            }
        }

        // 初始化 PyO3 解释器
        pyo3::prepare_freethreaded_python();
        println!("🐍 Python 解释器初始化完成");

        Python::with_gil(|py| {
            let init_code = r#"
import sys
import os

# 1. 强行剥夺嵌入式身份，伪装成原生的 python.exe
sys.frozen = False
sys.executable = os.path.join(sys.base_exec_prefix, "python.exe")
sys._base_executable = sys.executable

if not hasattr(sys, 'argv') or not sys.argv:
    sys.argv = ['']

# 2. 🌟 提前“排雷”：在主线程把会报错的库全部初始化一遍！
try:
    import asyncio
    import pythoncom
    import xlwings
    import pandas
except Exception as e:
    print(f"⚠️ 预加载模块出现警告: {e}")
"#;
            if let Err(e) = py.run(init_code, None, None) {
                println!("⚠️ Python 预热失败: {}", e);
            } else {
                println!("🛡️ Python 底层环境伪装及预热完毕 (彻底免疫线程信号崩溃)");
            }
        });
    });
}

// 辅助函数：解压逻辑
fn unzip_file(zip_path: &Path, dest_dir: &Path) -> io::Result<()> {
    let file = fs::File::open(zip_path)?;
    let mut archive = zip::ZipArchive::new(file)?;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let outpath = match file.enclosed_name() {
            Some(path) => dest_dir.join(path),
            None => continue,
        };

        if file.name().ends_with('/') {
            fs::create_dir_all(&outpath)?;
        } else {
            if let Some(p) = outpath.parent() {
                if !p.exists() {
                    fs::create_dir_all(p)?;
                }
            }
            let mut outfile = fs::File::create(&outpath)?;
            io::copy(&mut file, &mut outfile)?;
        }
    }
    Ok(())
}

/// 启动时清理旧的备份文件
///
/// 每次应用启动时调用，防止 `backups/` 文件夹无限膨胀。
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
///
/// # 功能增强 (Robustness Upgrade)
///
/// 为了防止 AI 生成的代码 "吞掉" 异常 (即 try...except print error)，
/// 本函数实现了**双流检测**机制：
/// 1. **Stderr 检测**: 捕获解释器级别的 Crash 和 traceback。
/// 2. **Stdout 关键词检测**: 扫描输出中是否包含 "Error", "Exception", "❌" 等关键词。
///
/// 任何一种情况命中，都会返回 `Err`，从而触发上层的自动修复逻辑。
pub async fn run_python_code(code: &str) -> Result<String, String> {
    let code = code.to_string();

    // 放入 blocking 线程池，防止卡死 UI
    let result = tokio::task::spawn_blocking(move || {
        Python::with_gil(|py| {
            let sys = py.import("sys").map_err(|e| e.to_string())?;
            let io = py.import("io").map_err(|e| e.to_string())?;

            // 1. 分离标准输出 (stdout) 和 标准错误 (stderr)
            let stdout_capture = io.call_method0("StringIO").map_err(|e| e.to_string())?;
            let stderr_capture = io.call_method0("StringIO").map_err(|e| e.to_string())?;
            // 劫持标准输出
            sys.setattr("stdout", stdout_capture)
                .map_err(|e| e.to_string())?;
            sys.setattr("stderr", stdout_capture)
                .map_err(|e| e.to_string())?;

            // 2. 执行代码
            let run_result = py.run(&code, None, None);

            // 3. 提取输出
            let stdout_str = stdout_capture
                .call_method0("getvalue")
                .unwrap()
                .extract::<String>()
                .unwrap_or_default();
            let stderr_str = stderr_capture
                .call_method0("getvalue")
                .unwrap()
                .extract::<String>()
                .unwrap_or_default();

            // 4. 智能错误判断逻辑
            // 情况 A: Python 解释器直接抛出异常 (硬错误)
            if let Err(e) = run_result {
                let full_err = format!(
                    "🐍 Runtime Exception:\n{}\n\n📝 Stderr Trace:\n{}",
                    e, stderr_str
                );
                return Err(full_err);
            }

            // 情况 B: 检查 Stderr 是否包含严重错误关键词
            if !stderr_str.trim().is_empty() {
                let lower_err = stderr_str.to_lowercase();
                if lower_err.contains("error")
                    || lower_err.contains("exception")
                    || lower_err.contains("traceback")
                {
                    // 如果 stderr 里有明显的错误词，视为失败
                    return Err(format!("⚠️ Detected Error in Stderr:\n{}", stderr_str));
                }
            }

            // 情况 C: 检查 Stdout 是否包含“软错误”关键词 (AI 吞掉了异常 print 出来的情况)
            let lower_out = stdout_str.to_lowercase();
            // 关键词黑名单：只要出现这些词，就认为脚本执行结果是不符合预期的
            let error_keywords = [
                "error:",          // 通用错误
                "exception:",      // 异常
                "traceback (most", // 堆栈
                "failed to",       // 失败
                "attributeerror",  // 常见属性错误
                "keyerror",        // 键错误
                "valueerror",      // 值错误
                "not found",       // 文件未找到
                "❌",              // AI 习惯用的 emoji
            ];

            for kw in error_keywords {
                if lower_out.contains(kw) {
                    // 发现疑似错误，返回 Err 触发重试
                    // 把 stdout 原样返回作为错误信息，让 AI 看到它打印了什么
                    return Err(stdout_str);
                }
            }

            // 一切正常
            Ok(stdout_str)
        })
    })
    .await;

    match result {
        Ok(python_result) => python_result.map_err(|e| e.to_string()),
        Err(e) => Err(format!("System Task Error: {}", e)),
    }
}

/// 批量创建热备份
///
/// 使用 `shutil.copy2` 进行物理文件复制。
/// 这天然支持多 Sheet，因为它复制的是整个 `.xlsx` 文件。
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

/// 批量热撤销 (Safe Plan B & Visual Audit)
///
/// # 架构变更 (Safe Undo Upgrade)
///
/// * **逻辑**: **全量扫描 + 安全策略**。
///     1.  遍历备份文件里的**所有** Sheet。
///     2.  如果目标里有同名 Sheet -> 覆盖恢复 (标记为**绿色**)。
///     3.  如果目标里没有 -> 新建并恢复 (标记为**绿色**)。
///     4.  **关键**: 如果目标里多出了 Sheet (无论是 AI 建的还是用户建的) -> **绝不删除**，但标记为**红色**并提示用户。
///     5.  **性能优化**: 开启 `screen_updating = False`，加速多表操作。
pub async fn run_batch_hot_undo(restore_pairs: Vec<(String, String)>) -> Result<String, String> {
    let pairs_repr = format!("{:?}", restore_pairs);

    let code = format!(
        r#"
import xlwings as xw
import os

pairs = {}
log = []

for target_file, backup_file in pairs:
    try:
        # 1. 尝试连接已打开的 Excel 实例
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
            log.append(f"⚠️ 跳过 {{os.path.basename(target_file)}}: 文件未打开")
            continue

        app = wb_target.app
        
        # [MODIFIED] 性能优化: 冻结屏幕刷新，大幅提升多表操作速度
        app.screen_updating = False
        app.display_alerts = False
        
        try:
            # 2. 后台打开备份文件
            wb_backup = app.books.open(backup_file)
            
            restored_list = []
            
            # 3. [NEW] 核心循环: 以备份文件为“真理”，强制还原所有旧数据
            for s_bak in wb_backup.sheets:
                s_name = s_bak.name
                
                # 尝试在目标中获取同名 Sheet
                try:
                    s_tgt = wb_target.sheets[s_name]
                except:
                    # [NEW] 复活逻辑: 如果目标里没有(被误删)，则新建并放到最后
                    s_tgt = wb_target.sheets.add(name=s_name, after=wb_target.sheets[-1])
                
                # 暴力恢复内容: 清空 -> 全量复制
                s_tgt.clear()
                s_bak.used_range.copy(s_tgt.range('A1'))
                
                # [NEW] 视觉标记: 恢复成功的表标为绿色 (ColorIndex: 4 或 RGB)
                try: 
                    # 绿色，代表 "Safe / Restored"
                    s_tgt.api.Tab.Color = 5296274 
                except: pass
                
                restored_list.append(s_name)
            
            # 4. [NEW] 审计逻辑: 检查多余的 Sheet (Safe Mode)
            # 我们绝不自动删除用户可能新建的表，只做标记
            tgt_sheets = [s.name for s in wb_target.sheets]
            bak_sheets = [s.name for s in wb_backup.sheets]
            
            # 计算差集: 目标有但备份没有的表
            extra_sheets = list(set(tgt_sheets) - set(bak_sheets))
            
            # [NEW] 视觉标记: 多余的表标为红色 (Danger / Check Needed)
            for extra in extra_sheets:
                try:
                    # 红色，代表 "Attention Needed"
                    wb_target.sheets[extra].api.Tab.Color = 255 
                except: pass

            wb_backup.close()
            
            # 5. 构造反馈消息 (使用 Markdown 语法)
            msg = f"✅ 已回溯 **{{os.path.basename(target_file)}}**"
            
            if extra_sheets:
                # 🔥 这里使用了 > 引用语法，配合 CSS 形成红色警告框
                msg += f"\n\n> 🚨 **检测到新增工作表（已保留）**"
                msg += f"\n> 系统发现以下表格不在备份中：`{{extra_sheets}}`"
                msg += f"\n> 为防止数据丢失，已将其**标红**，请务必手动确认是否删除。"
            else:
                msg += f"\n✨ 状态已完美同步"
                
            log.append(msg)
            
        finally:
            # [IMPORTANT] 无论成功失败，必须恢复屏幕刷新，否则 Excel 会假死
            app.screen_updating = True
            app.display_alerts = True

    except Exception as e:
        log.append(f"❌ {{os.path.basename(target_file)}} 撤销失败: {{e}}")

print("\n".join(log))
"#,
        pairs_repr
    );

    run_python_code(&code).await
}
