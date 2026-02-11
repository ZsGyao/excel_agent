# excel_core.py - 工业级 Excel 智能核心库
import os
import sys
import time
import json
import datetime
import pandas as pd
import xlwings as xw

# 🔥 强制注入 License，防止商业版环境报错
os.environ["XLWINGS_LICENSE_KEY"] = "non-commercial"

# ==========================================
# 1. 结构感知层 (探测表头范围)
# ==========================================

def _detect_header_by_color(sheet, scan_rows=15):
    """【策略 A】颜色锚点法：识别用户标记颜色的区域作为表头"""
    last_colored_row = 0
    for r in range(1, scan_rows + 1):
        try:
            # 只要 A 列有非白色背景，就视为表头行
            bg_color = sheet.range((r, 1)).color
            if bg_color and bg_color != (255, 255, 255):
                last_colored_row = r
        except: pass
    return last_colored_row

def _is_potential_header_row(row_values):
    """判断某一行是否具备表头特征（高文本占比）"""
    if not row_values: return False
    str_count = 0
    num_count = 0
    for cell in row_values:
        val = str(cell).strip()
        if not val or val.lower() == 'nan': continue
        try:
            float(val)
            num_count += 1
        except:
            str_count += 1
    total = str_count + num_count
    return (str_count / total) > 0.8 if total > 0 else True

def _detect_header_by_logic(sheet, scan_rows=15):
    """【策略 B】类型突变法：自动寻找文本区与数据区的分界线"""
    try:
        raw_rows = sheet.range((1,1), (scan_rows, 20)).value
        last_header_idx = 0
        for i in range(len(raw_rows) - 1):
            if _is_potential_header_row(raw_rows[i]) and not _is_potential_header_row(raw_rows[i+1]):
                last_header_idx = i
        return last_header_idx + 1
    except: return 1

# ==========================================
# 2. 执行层 (处理合并单元格与数据加载)
# ==========================================

def safe_load(file_path, config=None):
    """全自动加载：支持颜色识别、合并单元格填充、多级表头扁平化"""
    try:
        wb = xw.books[os.path.basename(file_path)]
    except:
        wb = xw.books.open(file_path)
    sheet = wb.sheets.active

    # A. 确定表头行数 (颜色优先 > 算法分析)
    h_count = _detect_header_by_color(sheet)
    if h_count == 0:
        h_count = _detect_header_by_logic(sheet)
        print(f"🤖 [自动识别] 基于数据模式判定表头为: {h_count} 行")
    else:
        print(f"🎨 [颜色识别] 基于用户标记判定表头为: {h_count} 行")
    
    # B. 读取包含表头和数据的全量矩阵
    last_cell = sheet.used_range.last_cell
    raw_values = sheet.range((1,1), (last_cell.row, last_cell.column)).value

    # C. 特殊处理：合并单元格的横向填充 (解决大项套小项，如“应知应会”)
    header_matrix = pd.DataFrame(raw_values[:h_count])
    # 🔥 关键：横向填充 None，确保存储合并单元格的完整路径
    header_matrix = header_matrix.ffill(axis=1) 
    
    # D. 构造多级语义列名 (如: "应知应会 - 食品")
    new_cols = []
    for col_idx in range(len(header_matrix.columns)):
        levels = header_matrix.iloc[:, col_idx].dropna().astype(str).str.strip().tolist()
        path = []
        for l in levels:
            if l and l != 'nan' and 'Unnamed' not in l and l not in path:
                path.append(l)
        new_cols.append(" - ".join(path) if path else f"Column_{col_idx+1}")

    # E. 组装数据主体
    df = pd.DataFrame(raw_values[h_count:], columns=new_cols)
    df.columns = [str(c).replace('\n', '') for c in df.columns] # 清洗换行符
    
    return wb, sheet, df, h_count + 1

# ==========================================
# 3. 报告层 (自动制表)
# ==========================================

def write_report(wb, title, data_list):
    """创建独立统计表，不干扰原数据"""
    ts = int(time.time())
    sheet_name = f"Stat_{ts}"
    ws = wb.sheets.add(sheet_name, after=wb.sheets[-1])
    
    content = [[title], [f"Generated: {time.ctime()}"], [""]] + data_list
    ws.range('A1').value = content
    ws.autofit()
    
    print(f"📄 统计结果已生成至新工作表: [{sheet_name}]")
    return sheet_name

def log_error(wb, msg):
    print(f"❌ 执行错误: {msg}")
    try:
        final_name = f"Error_{int(time.time())}"
        ws = wb.sheets.add(final_name)
        ws.range('A1').value = f"Trace: {msg}"
        ws.range('A1').font.color = (255, 0, 0)
    except: pass