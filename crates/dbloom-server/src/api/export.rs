//! 导出路由（export，02-api §2.8）：查询结果 → CSV/XLSX/JSON/SQL 落共享卷（D18）。
//!
//! - 共享根：`DBLOOM_SHARED_ROOT`（缺省 `./shared`），文件写到 `{root}/export/YYYY/MM/name.ext`；
//! - 返回相对共享根的路径（多节点共享卷表现为同一目录），下载走 `GET /api/v1/export/download`;
//! - 仅允许只读 SQL 导出（防用导出接口执行写操作）；
//! - 行数上限 `max_rows`（默认 100_000，上限 200_000）。

use axum::{
    Extension, Json,
    extract::{Query, State},
    http::header,
    http::HeaderMap,
};
use chrono::Datelike;
use dbloom_common::AppError;
use dbloom_connector::{SqlKind, classify_sql, query_rows};
use dbloom_types::ExportRequest;
use rust_xlsxwriter::{Format, Workbook, Worksheet};
use serde::Deserialize;
use serde_json::Value;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use crate::{
    auth::AuthCtx,
    error::{ok_json, ApiError},
    state::AppState,
};

/// 共享根目录（D18：所有节点挂同一共享卷）。
pub(crate) fn shared_root() -> PathBuf {
    PathBuf::from(
        std::env::var("DBLOOM_SHARED_ROOT")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "./shared".to_string()),
    )
}

/// POST /api/v1/export —— 执行只读查询并写 4 格式文件。
pub async fn create(
    State(state): State<Arc<AppState>>,
    ctx: Extension<AuthCtx>,
    Json(req): Json<ExportRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let sql = req.sql.trim();
    if sql.is_empty() {
        return Err(ApiError::from(AppError::validation("导出 SQL 不能为空")));
    }
    let (kind, _) = classify_sql(sql);
    if kind != SqlKind::Read {
        return Err(ApiError::from(AppError::validation(
            "导出仅支持只读查询（SELECT/SHOW 等），写/危险语句请到 SQL 工作台执行",
        )));
    }
    let (_row, pool) = crate::api::query::open_pool_for(&state, &ctx, req.connection_id).await?;
    let fmt = req.format.to_ascii_lowercase();
    if !matches!(fmt.as_str(), "csv" | "xlsx" | "json" | "sql") {
        return Err(ApiError::from(AppError::validation(
            "format 仅支持 csv/xlsx/json/sql",
        )));
    }
    let max_rows = req.max_rows.unwrap_or(100_000).clamp(1, 200_000) as usize;

    // 分页拉取（每页 5000，循环直至取完或达上限）
    let mut columns = Vec::new();
    let mut rows: Vec<Vec<Value>> = Vec::new();
    let mut page: i64 = 1;
    loop {
        let r = query_rows(&pool, sql, page, 5000, 120_000)
            .await
            .map_err(|e| ApiError::from(AppError::internal(format!("导出查询失败: {e}"))))?;
        if page == 1 {
            columns = r.columns;
        }
        rows.extend(r.rows);
        if !r.has_more || rows.len() >= max_rows {
            break;
        }
        page += 1;
    }
    rows.truncate(max_rows);

    let name = sanitize_name(
        req.file_name
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or("export"),
    );
    let rel = export_rel_path(&name, &fmt);
    let abs = shared_root().join(&rel);
    if let Some(parent) = abs.parent() {
        std::fs::create_dir_all(parent).map_err(|e| ApiError::from(AppError::internal(format!("创建导出目录失败: {e}"))))?;
    }

    let bytes = match fmt.as_str() {
        "csv" => render_csv(&columns, &rows)?,
        "json" => render_json(&columns, &rows)?,
        "sql" => render_sql(sql, &columns, &rows)?,
        "xlsx" => render_xlsx(&columns, &rows)?,
        _ => unreachable!(),
    };
    std::fs::write(&abs, &bytes).map_err(|e| ApiError::from(AppError::internal(format!("写导出文件失败: {e}"))))?;

    state
        .iam
        .record_audit(
            Some(ctx.user_id()),
            "user",
            "export",
            Some("export"),
            Some(&req.connection_id.to_string()),
            Some(serde_json::json!({ "format": fmt, "rows": rows.len(), "file": rel.to_string_lossy().to_string(), "bytes": bytes.len() })),
            None,
        )
        .await
        .map_err(ApiError::from)?;

    Ok(ok_json(serde_json::json!({
        "file": rel.to_string_lossy().to_string(),
        "rows": rows.len() as i64,
        "bytes": bytes.len() as i64,
    })))
}

/// 下载查询参数。
#[derive(Debug, Deserialize)]
pub struct DownloadReq {
    pub path: String,
}

/// GET /api/v1/export/download?path=export/2026/10/xxx.csv —— 校验共享 export 根内下载。
pub async fn download(
    State(_state): State<Arc<AppState>>,
    _ctx: Extension<AuthCtx>,
    Query(q): Query<DownloadReq>,
) -> Result<(HeaderMap, Vec<u8>), ApiError> {
    let root = shared_root();
    let export_root = root.join("export");
    // 路径安全：必须位于 {shared}/export/ 下（去重 `..` 后校验前缀）
    let rel = Path::new(&q.path);
    let mut clean = PathBuf::new();
    for comp in rel.components() {
        match comp {
            Component::Normal(c) => clean.push(c),
            Component::ParentDir | Component::CurDir | Component::RootDir | Component::Prefix(_) => {
                return Err(ApiError::from(AppError::validation("导出下载路径非法")));
            }
        }
    }
    let abs = root.join(&clean);
    let abs_str = abs.to_string_lossy().to_string();
    let export_str = export_root.to_string_lossy().to_string();
    if !abs_str.starts_with(&export_str) {
        return Err(ApiError::from(AppError::validation("导出下载路径越界")));
    }
    let bytes = tokio::fs::read(&abs)
        .await
        .map_err(|_| ApiError::from(AppError::not_found("导出文件不存在")))?;
    let fname = clean.file_name().unwrap_or_default().to_string_lossy().to_string();
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_DISPOSITION,
        format!("attachment; filename=\"{fname}\"").parse().map_err(|_| ApiError::from(AppError::internal("header 非法")))?,
    );
    headers.insert(
        header::CONTENT_TYPE,
        mime_for(&fname)
            .parse()
            .unwrap_or_else(|_| header::HeaderValue::from_static("application/octet-stream")),
    );
    Ok((headers, bytes))
}

// ---------------- 渲染 ----------------

fn render_csv(cols: &[dbloom_types::ColumnMeta], rows: &[Vec<Value>]) -> Result<Vec<u8>, ApiError> {
    let mut out = String::new();
    out.push_str(&cols.iter().map(|c| csv_escape(&c.name)).collect::<Vec<_>>().join(","));
    out.push('\n');
    for row in rows {
        let cells: Vec<String> = row.iter().map(cell_csv).collect();
        out.push_str(&cells.join(","));
        out.push('\n');
    }
    Ok(out.into_bytes())
}

fn cell_csv(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::Bool(b) => csv_escape(if *b { "true" } else { "false" }),
        Value::Number(n) => n.to_string(),
        Value::String(s) => csv_escape(s),
        other => csv_escape(&other.to_string()),
    }
}

fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') || s.contains('\r') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn render_json(cols: &[dbloom_types::ColumnMeta], rows: &[Vec<Value>]) -> Result<Vec<u8>, ApiError> {
    let payload = serde_json::json!({
        "columns": cols.iter().map(|c| &c.name).collect::<Vec<_>>(),
        "rows": rows,
    });
    serde_json::to_vec_pretty(&payload).map_err(|e| ApiError::from(AppError::internal(format!("JSON 序列化失败: {e}"))))
}

fn render_sql(_src: &str, cols: &[dbloom_types::ColumnMeta], rows: &[Vec<Value>]) -> Result<Vec<u8>, ApiError> {
    let table = "export_data";
    let col_list = cols.iter().map(|c| format!("`{}`", c.name)).collect::<Vec<_>>().join(", ");
    let mut out = String::new();
    for row in rows {
        let vals = row.iter().map(sql_val).collect::<Vec<_>>().join(", ");
        out.push_str(&format!("INSERT INTO `{table}` ({col_list}) VALUES ({vals});\n"));
    }
    Ok(out.into_bytes())
}

fn sql_val(v: &Value) -> String {
    match v {
        Value::Null => "NULL".to_string(),
        Value::Bool(b) => if *b { "1".into() } else { "0".into() },
        Value::Number(n) => n.to_string(),
        Value::String(s) => format!("'{}'", s.replace('\'', "''")),
        other => format!("'{}'", other.to_string().replace('\'', "''")),
    }
}

fn render_xlsx(cols: &[dbloom_types::ColumnMeta], rows: &[Vec<Value>]) -> Result<Vec<u8>, ApiError> {
    let mut wb = Workbook::new();
    let ws = wb.add_worksheet();
    let header = Format::new().set_bold();
    for (ci, c) in cols.iter().enumerate() {
        ws.write_string_with_format(0, ci as u16, &c.name, &header)
            .map_err(|e| ApiError::from(AppError::internal(format!("XLSX 写表头失败: {e}"))))?;
    }
    for (ri, row) in rows.iter().enumerate() {
        let r = (ri + 1) as u32;
        for (ci, v) in row.iter().enumerate() {
            write_xlsx_cell(ws, r, ci as u16, v)?;
        }
    }
    wb.save_to_buffer()
        .map_err(|e| ApiError::from(AppError::internal(format!("XLSX 生成失败: {e}"))))
}

fn write_xlsx_cell(ws: &mut Worksheet, r: u32, c: u16, v: &Value) -> Result<(), AppError> {
    let res = match v {
        Value::Null => ws.write_blank(r, c, &Format::default()),
        Value::Bool(b) => ws.write_boolean(r, c, *b),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                ws.write_number(r, c, i as f64)
            } else if let Some(f) = n.as_f64() {
                ws.write_number(r, c, f)
            } else {
                ws.write_string(r, c, &n.to_string())
            }
        }
        Value::String(s) => ws.write_string(r, c, s),
        other => ws.write_string(r, c, &other.to_string()),
    };
    res.map(|_| ())
        .map_err(|e| AppError::internal(format!("XLSX 写单元格失败: {e}")))
}

// ---------------- 工具 ----------------

fn sanitize_name(s: &str) -> String {
    let mut out = String::new();
    for ch in s.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' || ch == '.' {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        "export".to_string()
    } else {
        out
    }
}

fn export_rel_path(name: &str, fmt: &str) -> PathBuf {
    let now = chrono::Local::now();
    PathBuf::from(format!(
        "export/{:04}/{:02}/{name}.{fmt}",
        now.year(),
        now.month()
    ))
}

fn mime_for(fname: &str) -> String {
    let lower = fname.to_ascii_lowercase();
    if lower.ends_with(".csv") {
        "text/csv; charset=utf-8".into()
    } else if lower.ends_with(".json") {
        "application/json; charset=utf-8".into()
    } else if lower.ends_with(".xlsx") {
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet".into()
    } else if lower.ends_with(".sql") {
        "application/sql; charset=utf-8".into()
    } else {
        "application/octet-stream".into()
    }
}
