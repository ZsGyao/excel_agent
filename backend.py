import json
import os
import shutil
import sys

import pandas as pd


def get_columns(file_path):
    try:
        df = pd.read_excel(file_path, engine="openpyxl", nrows=0)
        return ",".join(df.columns.tolist())
    except Exception as e:
        return f"Error: {str(e)}"


def execute_analysis(file_path, code_snippet, temp_id):
    """
    执行代码，但只保存到临时文件。
    temp_id: Rust 传过来的唯一ID，用于区分不同的操作版本
    """
    response = {"status": "success", "message": "", "preview": None}

    # 临时文件路径：原文件名_temp_ID.xlsx
    # 例如: D:\data\test.xlsx -> D:\data\test_temp_abc123.xlsx
    temp_file_path = f"{file_path}_{temp_id}.tmp.xlsx"

    try:
        # 1. 加载原文件
        df = pd.read_excel(file_path, engine="openpyxl")

        # 2. 准备环境
        local_vars = {"df": df, "result": None, "pd": pd, "np": __import__("numpy")}

        # 3. 执行代码
        exec(code_snippet, {}, local_vars)

        # 4. 获取结果
        final_df = local_vars.get("df")
        custom_result = local_vars.get("result")

        # 5. 🔥 关键修改：保存到临时文件，而不是覆盖原文件
        final_df.to_excel(temp_file_path, index=False)

        # 6. 生成预览
        preview_json = final_df.head(10).to_json(orient="split", date_format="iso")
        response["preview"] = json.loads(preview_json)

        if custom_result is not None:
            response["message"] = f"计算结果: {str(custom_result)}"
        else:
            response["message"] = "✅以此为预览，请确认是否保存修改。"

    except Exception as e:
        response["status"] = "error"
        response["message"] = f"❌ Python 执行错误:\n{str(e)}"
        # 如果出错，清理可能产生的垃圾文件
        if os.path.exists(temp_file_path):
            os.remove(temp_file_path)

    return json.dumps(response, ensure_ascii=False)


def confirm_save(file_path, temp_id):
    """
    用户点击确认后，将临时文件覆盖回原文件
    """
    temp_file_path = f"{file_path}_{temp_id}.tmp.xlsx"

    try:
        if os.path.exists(temp_file_path):
            # 覆盖原文件
            shutil.move(temp_file_path, file_path)
            return "✅ 修改已成功保存到原文件！"
        else:
            return "❌ 错误：临时文件已丢失（可能已过期）。"
    except Exception as e:
        return f"❌ 保存失败: {str(e)}"


def discard_change(file_path, temp_id):
    """
    用户点击放弃，删除临时文件
    """
    temp_file_path = f"{file_path}_{temp_id}.tmp.xlsx"
    if os.path.exists(temp_file_path):
        os.remove(temp_file_path)
        return "🗑️ 已放弃修改。"
    return "已放弃。"


if __name__ == "__main__":
    pass
