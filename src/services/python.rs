use pyo3::prelude::*;
use pyo3::types::PyModule;
use std::env;
use std::fs;

/// 初始化 Python 环境路径
pub fn init_python_env() {
    let current_dir = env::current_dir().unwrap();
    let py_env_path = current_dir.join("py_env");

    let std_lib = py_env_path.join("python311.zip");
    let site_packages = py_env_path.join("Lib\\site-packages");
    let dlls = py_env_path.join("DLLs");

    let new_python_path = format!(
        "{};{};{}",
        std_lib.display(),
        site_packages.display(),
        dlls.display()
    );

    env::set_var("PYTHONHOME", &py_env_path);
    env::set_var("PYTHONPATH", &new_python_path);
    println!("Python 环境已配置: {:?}", py_env_path);
}

/// 获取 Excel 列名
pub fn get_excel_columns(file_path: &str) -> String {
    let current_dir = env::current_dir().unwrap();
    let backend_path = current_dir.join("backend.py");
    let backend_code = fs::read_to_string(backend_path).unwrap_or_default();

    Python::with_gil(|py| -> PyResult<String> {
        let backend = PyModule::from_code(py, &backend_code, "backend.py", "backend_module")?;
        let columns: String = backend
            .getattr("get_columns")?
            .call1((file_path,))?
            .extract()?;
        Ok(columns)
    })
    .unwrap_or_else(|e| format!("读取列名失败: {}", e))
}

/// 执行 Python 代码
pub fn run_python_code(file_path: &str, code: &str) -> String {
    let current_dir = env::current_dir().unwrap();
    let backend_path = current_dir.join("backend.py");
    let backend_code = fs::read_to_string(backend_path).unwrap_or_default();

    Python::with_gil(|py| -> PyResult<String> {
        let backend = PyModule::from_code(py, &backend_code, "backend.py", "backend_module")?;
        let result: String = backend
            .getattr("execute_analysis")?
            .call1((file_path, code))?
            .extract()?;
        Ok(result)
    })
    .unwrap_or_else(|e| format!("Python 执行出错: {}", e))
}
