import pythoncom
import atexit
import json
import traceback
import pandas as pd

# 1. 注册全局 COM 初始化
pythoncom.CoInitialize()

# 2. 注册安全退出钩子：无论脚本是正常结束还是报错崩溃，系统都会自动调用它释放 Excel 锁！
def cleanup_com():
    pythoncom.CoUninitialize()
atexit.register(cleanup_com)

# 3. 核心依赖加载
try:
    import xlwings as xw
except ImportError:
    print("❌ 缺少 xlwings 库，请检查环境。")
    exit(1)

# 4. 加载沙盒上下文
with open(".agent_context.json", "r", encoding="utf-8") as f:
    schema_json = json.load(f)

file_path = list(schema_json.keys())[0]
wb = xw.Book(file_path)

def load_safe_df(sheet_name):
    sheet_schema = schema_json[file_path]["sheets"][sheet_name]
    pure_data_start_row = sheet_schema["data_start_row"] + 1
    semantic_columns = sheet_schema["columns"]
    
    sheet = wb.sheets[sheet_name]
    safe_max_row = sheet_schema["data_start_row"] + sheet_schema["total_rows"] - 1
    max_row = min(sheet.used_range.last_cell.row, safe_max_row)
    
    if max_row < pure_data_start_row: return pd.DataFrame(columns=semantic_columns)
        
    df = sheet.range((pure_data_start_row, 1), (max_row, len(semantic_columns))).options(pd.DataFrame, header=False, index=False).value
    df = df.iloc[:, :len(semantic_columns)]
    df.columns = semantic_columns
    return df

def get_col_name(df, keyword):
    for col in df.columns:
        if str(col).split("@|||@")[-1].strip() == keyword.strip(): return col
    for col in df.columns:
        if keyword.strip() in str(col): return col
    return keyword

def safe_update_column(sheet_name, target_col_full_name, df_single_column):
    pure_start = schema_json[file_path]["sheets"][sheet_name]["data_start_row"] + 1
    col_idx = schema_json[file_path]["sheets"][sheet_name]["columns"].index(target_col_full_name) + 1
    # 强制转为 DataFrame 写入，彻底根除 Numpy ambiguous truth value 报错
    wb.sheets[sheet_name].range((pure_start, col_idx)).options(index=False, header=False).value = pd.DataFrame(df_single_column)

def safe_create_report(report_name, final_df):
    try:
        new_sheet = wb.sheets.add(report_name)
    except Exception:
        new_sheet = wb.sheets[report_name]
        new_sheet.clear()
    new_sheet.range("A1").options(index=False, header=True).value = final_df

print("⏳ 正在安全连接 Excel 数据引擎...")

# ==========================================
# 🤖 AI 业务代码生成区 (0 缩进安全隔离)
# ==========================================
{{AI_BUSINESS_CODE}}
# ==========================================

print("✨ Excel 复合任务热更新完成！")