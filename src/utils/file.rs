use std::fs::OpenOptions;
use std::path::Path;

/// 探测 Excel 文件是否正在被用户打开（被其他进程占用写入锁）
pub fn is_file_locked(path: &str) -> bool {
    let path = Path::new(path);

    // 如果文件压根不存在，那肯定没被占用
    if !path.exists() {
        return false;
    }

    // 尝试以写入模式打开文件
    // 不修改文件内容，只是借此探测操作系统的锁状态
    match OpenOptions::new().write(true).append(true).open(path) {
        Ok(_) => {
            // 成功拿到了写入权限！说明用户没有打开这个 Excel
            // 变量离开作用域时，Rust 会自动 Drop 并关闭这个句柄，干净利落
            false
        }
        Err(_) => {
            // 拿不到写入权限！被 Excel 进程锁死了
            true
        }
    }
}
