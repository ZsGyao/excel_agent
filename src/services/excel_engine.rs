use calamine::{open_workbook, Data, Range, Reader, Xlsx};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// --- 1. 数据模型定义 (层级化 Schema) ---

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ColumnMapping {
    pub semantic_name: String, // 缝合后的名字，如 "应知应会 - 食品"
    pub physical_index: usize, // 物理列号 (0-based)
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SheetSchema {
    pub sheet_name: String,
    pub columns: Vec<ColumnMapping>,
    pub data_start_row: usize, // 数据起始行号 (0-based)
    pub total_rows: usize,
    pub total_cols: usize,
    pub preview_data: Vec<HashMap<String, String>>, // 数据预览
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FileSchema {
    pub file_path: String,
    pub sheets: HashMap<String, SheetSchema>, // 支持多 Sheet
}

// ----------------------- 2. 核心解析逻辑 -------------------------------

pub const SHEET_JOIN_STR: &str = "@|||@";

pub struct ExcelEngine;

impl ExcelEngine {
    /// 解析单个 Excel 文件中的所有 Sheet
    pub fn parse_file(file_path: &str, header_rows: usize) -> Result<FileSchema, String> {
        let mut workbook: Xlsx<_> =
            open_workbook(file_path).map_err(|e| format!("无法打开 Excel 文件: {}", e))?;

        let sheet_names = workbook.sheet_names().to_vec();
        let mut sheets = HashMap::new();

        for sheet_name in sheet_names {
            if let Ok(range) = workbook.worksheet_range(&sheet_name) {
                let schema = Self::parse_sheet(&sheet_name, &range, header_rows);
                sheets.insert(sheet_name, schema);
            }
        }

        Ok(FileSchema {
            file_path: file_path.to_string(),
            sheets,
        })
    }

    /// 精准解析接口。接收前端传来的精确配置：每个 Sheet 对应几行表头。
    /// `sheet_configs` 的格式例如：{"报名表": 3, "花名册": 2}
    pub fn parse_file_with_config(
        file_path: &str,
        sheet_configs: &HashMap<String, usize>,
    ) -> Result<FileSchema, String> {
        let mut workbook: Xlsx<_> =
            open_workbook(file_path).map_err(|e| format!("无法打开 Excel 文件: {}", e))?;

        let mut sheets = HashMap::new();

        // 只解析前端传过来的、用户确认需要导入的 Sheet
        for (sheet_name, &header_rows) in sheet_configs {
            if let Ok(range) = workbook.worksheet_range(sheet_name) {
                // 调用我们写好的 parse_sheet 核心算法
                let schema = Self::parse_sheet(sheet_name, &range, header_rows);
                sheets.insert(sheet_name.clone(), schema);
            }
        }

        Ok(FileSchema {
            file_path: file_path.to_string(),
            sheets,
        })
    }

    /// 预读接口, 只读取 Sheet 名字，不解析数据，用于渲染导入弹窗。
    pub fn get_sheet_names(file_path: &str) -> Result<Vec<String>, String> {
        let workbook: Xlsx<_> =
            open_workbook(file_path).map_err(|e| format!("无法打开 Excel 文件: {}", e))?;

        Ok(workbook.sheet_names().to_vec())
    }

    /// 核心算法：解析单个 Sheet 的表头并执行双向填充 (ffill)
    fn parse_sheet(sheet_name: &str, range: &Range<Data>, header_rows: usize) -> SheetSchema {
        let width = range.width();
        let height = range.height();

        // 取实际行数和预设表头行数的较小值，防止越界
        let actual_header_rows = std::cmp::min(height, header_rows);

        // 只有当表格完全为空时，才跳过解析
        if width == 0 || actual_header_rows == 0 {
            return SheetSchema {
                sheet_name: sheet_name.to_string(),
                columns: vec![],
                data_start_row: 0,
                total_rows: height,
                total_cols: width,
                preview_data: vec![],
            };
        }

        // 1. 提取前 N 行作为表头矩阵
        let mut matrix = vec![vec![String::new(); width]; actual_header_rows];
        for r in 0..actual_header_rows {
            for c in 0..width {
                if let Some(val) = range.get_value((r as u32, c as u32)) {
                    matrix[r][c] = val.to_string().trim().to_string();
                }
            }
        }

        // 2. 纵向填充 (axis=0) - 解决“姓名”等跨行合并
        for c in 0..width {
            for r in 1..actual_header_rows {
                if matrix[r][c].is_empty() {
                    matrix[r][c] = matrix[r - 1][c].clone();
                }
            }
        }

        // 3. 横向填充 (axis=1) - 解决“应知应会”等跨列合并
        for r in 0..actual_header_rows {
            for c in 1..width {
                if matrix[r][c].is_empty() {
                    matrix[r][c] = matrix[r][c - 1].clone();
                }
            }
        }

        // 4. 路径缝合与去重
        let mut columns = Vec::new();
        for c in 0..width {
            let mut path_nodes = Vec::new();
            for r in 0..actual_header_rows {
                let node = &matrix[r][c];
                // 排除空白项，去重 (例如 ["姓名", "姓名"] -> ["姓名"])
                if !node.is_empty() && !path_nodes.contains(node) {
                    path_nodes.push(node.clone());
                }
            }

            let semantic_name = if path_nodes.is_empty() {
                format!("Unnamed_Col_{}", c)
            } else {
                path_nodes.join(SHEET_JOIN_STR)
            };

            columns.push(ColumnMapping {
                semantic_name,
                physical_index: c,
            });
        }

        let mut preview_data = Vec::new();
        let preview_limit = 3;
        let actual_data_rows = std::cmp::min(height - actual_header_rows, preview_limit);

        for i in 0..actual_data_rows {
            let r = actual_header_rows + i; // 真实数据的行号
            let mut row_dict = HashMap::new();

            for c in 0..width {
                if let Some(val) = range.get_value((r as u32, c as u32)) {
                    let cell_str = val.to_string().trim().to_string();
                    if !cell_str.is_empty() {
                        let semantic_key = &columns[c].semantic_name;
                        row_dict.insert(semantic_key.clone(), cell_str);
                    }
                }
            }

            if !row_dict.is_empty() {
                preview_data.push(row_dict);
            }
        }

        SheetSchema {
            sheet_name: sheet_name.to_string(),
            columns,
            data_start_row: actual_header_rows, // 0-based
            total_rows: height,
            total_cols: width,
            preview_data,
        }
    }
}

// ==========================================
// 单元测试模块
// ==========================================
#[cfg(test)]
mod tests {
    use super::*;
    use calamine::{Data, Range};

    /// 辅助函数：快速创建一个内存中的 Range<Data>
    fn create_mock_range(rows: usize, cols: usize, data: Vec<((u32, u32), &str)>) -> Range<Data> {
        let mut range = Range::new((0, 0), ((rows - 1) as u32, (cols - 1) as u32));
        for ((r, c), val) in data {
            range.set_value((r, c), Data::String(val.to_string()));
        }
        range
    }

    #[test]
    fn test_parse_sheet_horizontal_merge() {
        // 测试场景：横向合并 (如“应知应会”跨两列)
        // [ "应知应会", (空),   "通识类" ]
        // [ "食品",     "特设", "法制"   ]
        let range = create_mock_range(
            2,
            3,
            vec![
                ((0, 0), "应知应会"),
                ((0, 2), "通识类"),
                ((1, 0), "食品"),
                ((1, 1), "特设"),
                ((1, 2), "法制"),
            ],
        );

        let schema = ExcelEngine::parse_sheet("TestSheet", &range, 2);

        assert_eq!(schema.columns.len(), 3);
        assert_eq!(schema.columns[0].semantic_name, "应知应会 - 食品");
        assert_eq!(schema.columns[1].semantic_name, "应知应会 - 特设");
        assert_eq!(schema.columns[2].semantic_name, "通识类 - 法制");
        assert_eq!(schema.data_start_row, 2);
    }

    #[test]
    fn test_parse_sheet_vertical_merge() {
        // 测试场景：纵向合并 (如“姓名”跨两行)
        // [ "序号", "姓名", "应知应会" ]
        // [ (空),   (空),   "食品"     ]
        let range = create_mock_range(
            2,
            3,
            vec![
                ((0, 0), "序号"),
                ((0, 1), "姓名"),
                ((0, 2), "应知应会"),
                ((1, 2), "食品"),
            ],
        );

        let schema = ExcelEngine::parse_sheet("TestSheet", &range, 2);

        assert_eq!(schema.columns.len(), 3);
        assert_eq!(schema.columns[0].semantic_name, "序号");
        assert_eq!(schema.columns[1].semantic_name, "姓名");
        assert_eq!(schema.columns[2].semantic_name, "应知应会 - 食品");
    }

    #[test]
    fn test_parse_sheet_complex_mixed() {
        // 测试场景：3层表头，包含空格、重复值和多重合并
        // [ "大类A", (空),   (空)   ]
        // [ "中类1", (空),   "中类2" ]
        // [ "小类a", "小类b", "小类a" ] // 注意这里有同名小类，路径应该能区分
        let range = create_mock_range(
            3,
            3,
            vec![
                ((0, 0), "大类A"),
                ((1, 0), "中类1"),
                ((1, 2), "中类2"),
                ((2, 0), "小类a"),
                ((2, 1), "小类b"),
                ((2, 2), "小类a"),
            ],
        );

        let schema = ExcelEngine::parse_sheet("TestSheet", &range, 3);

        assert_eq!(schema.columns.len(), 3);
        assert_eq!(schema.columns[0].semantic_name, "大类A - 中类1 - 小类a");
        assert_eq!(schema.columns[1].semantic_name, "大类A - 中类1 - 小类b");
        assert_eq!(schema.columns[2].semantic_name, "大类A - 中类2 - 小类a");
    }

    #[test]
    fn test_parse_sheet_empty_handling() {
        // 测试场景：空表或数据行数少于表头行数，不能发生 Panic
        let range = Range::new((0, 0), (0, 0)); // 只有 1x1 且为空
        let schema = ExcelEngine::parse_sheet("Empty", &range, 3);

        // 应该安全返回空列或者仅处理存在的部分，不报错
        assert_eq!(schema.total_rows, 1);
        assert_eq!(schema.columns[0].semantic_name, "Unnamed_Col_0");
    }
}
