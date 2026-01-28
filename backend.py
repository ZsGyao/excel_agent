import json
import sys

import pandas as pd


def get_columns(file_path):
    try:
        df = pd.read_excel(file_path, engine="openpyxl", nrows=0)
        return ",".join(df.columns.tolist())
    except Exception as e:
        return f"Error: {str(e)}"


def execute_analysis(file_path, code_snippet):
    """
    返回 JSON 字符串:
    {
        "status": "success" | "error",
        "message": "...",
        "preview": { "columns": [...], "data": [...] } (Optional)
    }
    """
    response = {"status": "success", "message": "", "preview": None}

    try:
        # 1. 加载 Excel
        df = pd.read_excel(file_path, engine="openpyxl")

        # 2. 准备环境
        local_vars = {"df": df, "result": None, "pd": pd, "np": __import__("numpy")}

        # 3. 执行代码
        exec(code_snippet, {}, local_vars)

        # 4. 获取结果
        final_df = local_vars.get("df")
        custom_result = local_vars.get("result")

        # 5. 保存文件 (这里还是先直接保存，后续我们再做 '另存为')
        final_df.to_excel(file_path, index=False)

        # 6. 生成预览数据 (JSON 格式)
        # orient='split' 会生成 {"columns": [...], "data": [...], "index": [...]}
        preview_json = final_df.head(10).to_json(orient="split", date_format="iso")
        response["preview"] = json.loads(preview_json)

        if custom_result is not None:
            response["message"] = f"计算结果: {str(custom_result)}"
        else:
            response["message"] = "✅ 处理成功，文件已更新。数据预览如下："

    except Exception as e:
        response["status"] = "error"
        response["message"] = f"❌ Python 执行错误:\n{str(e)}"

    # 返回 JSON 字符串给 Rust
    return json.dumps(response, ensure_ascii=False)


if __name__ == "__main__":
    pass
