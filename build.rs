fn main() {
    // 仅在 Windows 上执行
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap() == "windows" {
        let mut res = winres::WindowsResource::new();
        // 指向你的 icon.ico 文件
        // 注意：路径是相对于 Cargo.toml 的
        res.set_icon("assets/icon.ico");
        res.compile().unwrap();
    }
}
