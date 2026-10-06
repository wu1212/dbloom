//! 查询 / 元数据 / 行编辑 / 导出 DTO（`docs/design/02-api.md` §2.5–2.8）。
//!
//! 安全约定（D6 写保护）：
//! - 查询结果统一为 `serde_json::Value` 单元格（数值按 JSON 数字、其余字符化）；
//! - 写操作（UPDATE/DELETE/INSERT 等）默认需 `confirm=true` 二次确认；
//!   `needConfirm` 字段返回拦截原因，前端弹确认框后带 `confirm=true` 重发；
//! - 生产连接（`is_production`）+ `read_only_lock` 一律拒绝写操作（无需 confirm）。

use serde::{Deserialize, Serialize};

/// 列元信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnMeta {
    pub name: String,
    pub type_name: String,
    pub nullable: bool,
}

/// SQL 执行请求。
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryRequest {
    pub connection_id: i64,
    pub sql: String,
    /// 页号（1-based，默认 1）。
    #[serde(default)]
    pub page: Option<i64>,
    /// 每页行数（1..=5000，默认 100）。
    #[serde(default)]
    pub page_size: Option<i64>,
    /// 超时毫秒（默认 120_000，上限 120_000）。
    #[serde(default)]
    pub timeout_ms: Option<i64>,
    /// 写保护二次确认（D6）：危险/写语句需 true。
    #[serde(default)]
    pub confirm: Option<bool>,
}

/// SQL 执行结果（读：分页行集；写：affectedRows）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryResult {
    pub success: bool,
    pub columns: Vec<ColumnMeta>,
    pub rows: Vec<Vec<serde_json::Value>>,
    /// 本页总行数（读语句分页后）。
    pub total: i64,
    pub page: i64,
    pub page_size: i64,
    pub has_more: bool,
    pub exec_ms: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub affected_rows: Option<i64>,
    /// 写保护：非空 = 需 confirm=true 二次确认重发（值为拦截说明）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub need_confirm: Option<String>,
    /// 写保护：本条是否已确认通过。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confirmed: Option<bool>,
}

// ---------------- 元数据（schema，02-api §2.6） ----------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseItem {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableItem {
    pub name: String,
    pub table_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnItem {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_value: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    pub is_primary: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableDdl {
    pub database: String,
    pub table: String,
    pub ddl: String,
}

// ---------------- 行浏览 / 编辑（data，02-api §2.7，D6 写保护） ----------------

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RowListRequest {
    pub connection_id: i64,
    pub table: String,
    #[serde(default)]
    pub page: Option<i64>,
    #[serde(default)]
    pub page_size: Option<i64>,
}

/// 单键值（表列名 + 单元格值）。
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RowKey {
    pub column: String,
    pub value: serde_json::Value,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RowUpdateRequest {
    pub connection_id: i64,
    pub table: String,
    /// WHERE 键（通常是主键）。
    pub keys: Vec<RowKey>,
    /// SET 值。
    pub values: Vec<RowKey>,
    #[serde(default)]
    pub confirm: Option<bool>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RowDeleteRequest {
    pub connection_id: i64,
    pub table: String,
    pub keys: Vec<RowKey>,
    #[serde(default)]
    pub confirm: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RowWriteResult {
    pub success: bool,
    pub affected_rows: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub need_confirm: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confirmed: Option<bool>,
}

// ---------------- 导出（export，02-api §2.8，落共享卷 D18） ----------------

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportRequest {
    pub connection_id: i64,
    pub sql: String,
    /// csv | xlsx | json | sql。
    pub format: String,
    /// 文件名主体（不含扩展名）；缺省 = 首列/`query` + 时间戳。
    #[serde(default)]
    pub file_name: Option<String>,
    /// 行数上限（默认 100_000，上限 200_000）。
    #[serde(default)]
    pub max_rows: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    /// 相对共享根的路径，如 `export/2026/10/xxx.csv`。
    pub file: String,
    pub rows: i64,
    pub bytes: i64,
}
