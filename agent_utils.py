import os

# 🔥 核心修复：强制注入非商业版 License
# 这能解决 "Invalid xlwings license key" 报错
os.environ["XLWINGS_LICENSE_KEY"] = "non-commercial"

import pandas as pd
import xlwings as xw

def load_data(file_path):
    """
    通用加载入口：自动连接 -> 防截断读取 -> 智能表头缝合 -> 清洗列名
    返回: (wb, sheet, df, data_start_row_index)
    """
    # 1. 智能连接
    try:
        # 尝试连接已打开的文件
        wb = xw.books[os.path.basename(file_path)]
    except:
        # 否则打开文件
        wb = xw.books.open(file_path)
    sheet = wb.sheets.active

    # 2. 防截断读取 (Force Read UsedRange)
    last_cell = sheet.used_range.last_cell
    # 这里的 (1,1) 代表 A1
    raw_data = sheet.range((1,1), (last_cell.row, last_cell.column)).value
    df_raw = pd.DataFrame(raw_data)

    # 3. 智能表头算法
    header_row, data_start_idx = _smart_header_logic(df_raw)

    # 4. 构建最终 DataFrame
    df = df_raw.iloc[data_start_idx:].copy()
    df.columns = header_row
    # 清洗：去空格、去换行、重名处理
    df.columns = [str(c).strip().replace('\n', '') if c else f"Unnamed_{i}" for i, c in enumerate(df.columns)]
    df.reset_index(drop=True, inplace=True)

    return wb, sheet, df, data_start_idx

def _smart_header_logic(df):
    """
    核心算法：寻找真实表头，并处理中国式多级表头（合并单元格）
    """
    # A. 寻找“非空值”最多的一行作为【候选行】
    # 只看前 15 行，避免扫描全表
    sample = df.head(15)
    counts = sample.count(axis=1)
    if counts.empty: return [], 1
    
    candidate_idx = int(counts.idxmax())

    # B. 【回溯机制】检查候选行的上一行
    # 如果候选行是“消保、食品”（子标题），上一行是“姓名、部门”（主标题）
    # 那么主标题行才是真正的起始点
    keys = ['姓名', '部门', '序号', 'ID', '名称', 'Name', 'Dept']
    final_header_idx = candidate_idx
    
    if candidate_idx > 0:
        prev_row = df.iloc[candidate_idx - 1].astype(str).values.tolist()
        # 只要上一行包含任意一个关键词，就向上回溯
        if any(k in str(v) for v in prev_row for k in keys):
            final_header_idx = candidate_idx - 1

    # C. 【缝合机制】Parent Row + Child Row
    # 将 final_header_idx (父) 和 next_row (子) 合并
    header_row = df.iloc[final_header_idx].fillna('').astype(str).tolist()
    data_start = final_header_idx + 1

    if final_header_idx + 1 < len(df):
        next_row = df.iloc[final_header_idx + 1].fillna('').astype(str).tolist()
        
        # 判断下一行是否真的是子表头（非空值也得比较多才算）
        next_count = df.iloc[final_header_idx + 1].count()
        if next_count > 1: 
            stitched = []
            for i, parent in enumerate(header_row):
                child = next_row[i] if i < len(next_row) else ''
                p_str = parent.strip().replace('\n', '')
                c_str = child.strip().replace('\n', '')
                
                # 优先用子标题，子标题为空则继承父标题
                if c_str: stitched.append(c_str)
                elif p_str: stitched.append(p_str)
                else: stitched.append(f"Unnamed_{i}")
            
            header_row = stitched
            data_start = final_header_idx + 2 # 数据跳过子标题行

    return header_row, data_start