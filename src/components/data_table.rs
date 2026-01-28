use dioxus::{
    html::{table, thead},
    prelude::*,
};

use crate::models::TableData;

#[component]
pub fn DataTable(data: TableData) -> Element {
    rsx!(div {
        class: "table-container",
        table {
            thead {
                tr {
                    for col in data.columns.iter() {
                        th {"{col}"}
                    }
                }
            }
            tbody {
                for row in data.data.iter() {
                    tr {
                        for cell in row.iter() {
                            // Handle different data, remove ""
                            td { "{cell.to_string().trim_matches('\"')}" }
                        }
                    }
                }
            }
        }
    })
}
