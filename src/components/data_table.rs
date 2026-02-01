use dioxus::prelude::*;

#[derive(PartialEq, Clone, Props)]
pub struct TableData {
    pub headers: Vec<String>,
    pub data: Vec<Vec<String>>,
}

#[component]
pub fn DataTable(data: TableData) -> Element {
    rsx! {
        div { class: "table-container",
            table {
                thead {
                    tr {
                        for header in data.headers.iter() {
                            th { "{header}" }
                        }
                    }
                }
                tbody {
                    // 🔥 修复 E0282: 明确闭包参数类型
                    for row in data.data.iter() {
                        tr {
                            for cell in row.iter() {
                                td { "{cell}" }
                            }
                        }
                    }
                }
            }
        }
    }
}
