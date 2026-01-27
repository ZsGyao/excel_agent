import io
import sys

import pandas as pd


def get_columns(file_path):
    """
    只读取表头，告诉 Rust 有哪些列
    """
    try:
        # nrows=0 只读表头，速度极快
        df = pd.read_excel(file_path, engine="openpyxl", nrows=0)
        return ",".join(df.columns.tolist())
    except Exception as e:
        return f"Error: {str(e)}"


def execute_analysis(file_path, code_snippet):
    """
    Rust 会调用这个函数。
    :param file_path: Excel 文件的绝对路径
    :param code_snippet: AI 生成的 Pandas 代码
    :return: 处理结果的字符串摘要
    """
    try:
        # 1. 加载 Excel
        df = pd.read_excel(file_path, engine="openpyxl")

        # 2. 准备执行环境
        local_vars = {"df": df, "result": None}

        # 3. 执行 AI 生成的代码
        exec(code_snippet, {}, local_vars)

        # 4. 获取结果
        final_df = local_vars.get("df")
        custom_result = local_vars.get("result")

        # --- 🟢 关键修改：将修改后的 df 写回 Excel 文件 ---
        # 如果你不想覆盖原文件，可以改成 file_path.replace(".xlsx", "_new.xlsx")
        final_df.to_excel(file_path, index=False)
        # ------------------------------------------------

        if custom_result is not None:
            return f"计算结果:\n{custom_result}"

        # 返回前 5 行数据作为预览
        return f"✅ 文件已保存!\n处理成功，数据预览:\n{final_df.head().to_string()}"

    except Exception as e:
        return f"❌ Python 执行错误:\n{str(e)}"


if __name__ == "__main__":
    print("这是被 Rust 调用的模块，请不要直接运行。")
