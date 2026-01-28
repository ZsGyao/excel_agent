use pyo3::prelude::*;
use pyo3::types::PyModule;
use pyo3::types::PyTuple;
use std::env;
use std::fs;

/// Init Python Environment Path
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

/// Get Excel Columns Name
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

/// Execute Python Code
pub fn run_python_code(file_path: &str, code: &str, temp_id: &str) -> String {
    run_python_script("execute_analysis", (file_path, code, temp_id))
}

/// Confirm Save
pub fn confirm_save(file_path: &str, temp_id: &str) -> String {
    run_python_script("confirm_save", (file_path, temp_id))
}

/// Confirm Discard Chaneg
pub fn discard_change(file_path: &str, temp_id: &str) -> String {
    run_python_script("discard_change", (file_path, temp_id))
}

/// Inner Aux Fun
fn run_python_script(func_name: &str, args: impl IntoPy<Py<PyTuple>>) -> String {
    let current_dir = env::current_dir().unwrap();
    let backend_path = current_dir.join("backend.py");
    let backend_code = fs::read_to_string(backend_path).unwrap_or_default();

    Python::with_gil(|py| -> PyResult<String> {
        let backend = PyModule::from_code(py, &backend_code, "backend.py", "backend_module")?;
        let result: String = backend.getattr(func_name)?.call1(args)?.extract()?;
        Ok(result)
    })
    .unwrap_or_else(|e| format!("Python 调用失败: {}", e))
}
