import pandas as pd
import numpy as np

# 制造 Sheet1：带有“合并单元格”的恶心表头，以及缺考的空值 (NaN)
columns = pd.MultiIndex.from_tuples([
    ("基本信息", "姓名"),
    ("基本信息", "部门"),
    ("2026年度大考", "理论成绩"),
    ("2026年度大考", "实操成绩"),
    ("2026年度大考", "学习状态")
])

data1 = [
    ["张三", "销售部", 85, 90, "已完成"],
    ["李四", "研发部", 55, 80, "已完成"],
    ["王五", "销售部", np.nan, 75, "缺考"], # 故意留空
    ["赵六", "人事部", 95, 95, "已完成"],
    ["孙七", "研发部", 60, 50, "补考中"],
]
df1 = pd.DataFrame(data1, columns=columns)

# 制造 Sheet2：普通的单层表头，用于测试 VLOOKUP 跨表联查
data2 = [
    ["销售部", "马总", "3F"],
    ["研发部", "牛总", "5F"],
    ["人事部", "刘总", "2F"]
]
df2 = pd.DataFrame(data2, columns=["部门", "负责人", "所在楼层"])

# 写入 Excel
with pd.ExcelWriter("agent_test.xlsx") as writer:
    # ⚠️ 修复点：去掉了 df1 的 index=False，允许它写入行索引以支持多级表头
    df1.to_excel(writer, sheet_name="员工考核表") 
    
    # df2 是普通单层表头，依然可以使用 index=False
    df2.to_excel(writer, sheet_name="部门映射表", index=False)

print("✨ 测试文件 agent_test.xlsx 已成功生成！")