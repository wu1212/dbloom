//! 查询执行（D17 统一转发：server 端连接池 → 数据源执行 → JSON 单元格）。
//!
//! - 连接池：每次查询按 `ConnParams` 建池（进程级缓存由上层 TODO：连接地址相同则复用）；
//! - 读语句：`LIMIT/OFFSET` 分页，每页上限 5000 行，逐行流式取 `limit+1` 行判 has_more；
//! - 写语句：`execute` 返回 affected_rows（**需先过 D6 写保护**，本层不做授权判定）；
//! - 超时：默认 120s（`tokio::time::timeout`），超时即中断该次查询；
//! - 值解码：按数据库类型名分派 → `serde_json::Value`（数字/布尔/JSON/字符串/字节兜底）。

use crate::ConnParams;
use dbloom_types::{ColumnMeta, QueryResult};
use serde_json::json;
use sqlx::mysql::{MySqlConnectOptions, MySqlPool, MySqlPoolOptions, MySqlSslMode};
use sqlx::postgres::{PgConnectOptions, PgPool, PgPoolOptions, PgSslMode};
use sqlx::{Column as _, Row as _, TypeInfo, ValueRef};
use std::time::Duration;

/// 默认查询超时（毫秒）。
pub const DEFAULT_TIMEOUT_MS: i64 = 120_000;
/// 每页行数上限（D：5000 行）。
pub const MAX_PAGE_SIZE: i64 = 5_000;

/// 单库连接池句柄。
#[derive(Debug, Clone)]
pub enum DbPool {
    MySql(MySqlPool),
    Postgres(PgPool),
}

impl DbPool {
    pub fn mysql(&self) -> Option<&MySqlPool> {
        match self {
            DbPool::MySql(p) => Some(p),
            _ => None,
        }
    }
    pub fn postgres(&self) -> Option<&PgPool> {
        match self {
            DbPool::Postgres(p) => Some(p),
            _ => None,
        }
    }
}

/// 按连接参数建池（连接数 8；acquire 5s）。
pub async fn open_pool(p: &ConnParams) -> Result<DbPool, String> {
    match p.conn_type.as_str() {
        "mysql" => {
            let mut opts = MySqlConnectOptions::new()
                .host(&p.host)
                .port(p.port.unwrap_or(3306) as u16)
                .username(p.username.as_deref().unwrap_or(""))
                .password(p.password.as_deref().unwrap_or(""));
            if let Some(db) = &p.database {
                if !db.is_empty() {
                    opts = opts.database(db);
                }
            }
            opts = match p.ssl_mode.as_str() {
                "require" | "verify-ca" | "verify-full" => opts.ssl_mode(MySqlSslMode::Required),
                _ => opts.ssl_mode(MySqlSslMode::Disabled),
            };
            let pool = MySqlPoolOptions::new()
                .max_connections(8)
                .acquire_timeout(Duration::from_secs(5))
                .connect_with(opts)
                .await
                .map_err(|e: sqlx::Error| crate::clean_err(&e.to_string()))?;
            Ok(DbPool::MySql(pool))
        }
        "postgres" => {
            let mut opts = PgConnectOptions::new()
                .host(&p.host)
                .port(p.port.unwrap_or(5432) as u16)
                .username(p.username.as_deref().unwrap_or("postgres"))
                .password(p.password.as_deref().unwrap_or(""));
            if let Some(db) = &p.database {
                if !db.is_empty() {
                    opts = opts.database(db);
                }
            }
            opts = match p.ssl_mode.as_str() {
                "require" | "verify-ca" | "verify-full" => opts.ssl_mode(PgSslMode::Require),
                _ => opts.ssl_mode(PgSslMode::Disable),
            };
            let pool = PgPoolOptions::new()
                .max_connections(8)
                .acquire_timeout(Duration::from_secs(5))
                .connect_with(opts)
                .await
                .map_err(|e: sqlx::Error| crate::clean_err(&e.to_string()))?;
            Ok(DbPool::Postgres(pool))
        }
        other => Err(format!("连接类型 {other} 的查询能力将在对应驱动点亮后提供（manifest.implemented=false）")),
    }
}

/// 读语句：分页执行并返回 JSON 单元格行集 + 是否还有下一页。
pub async fn query_rows(
    pool: &DbPool,
    sql: &str,
    page: i64,
    page_size: i64,
    timeout_ms: i64,
) -> Result<QueryResult, String> {
    let page = page.max(1);
    let page_size = page_size.clamp(1, MAX_PAGE_SIZE);
    let start = dbloom_common::time::now_ms();

    let fut = async {
        match pool {
            DbPool::MySql(p) => exec_paged_mysql(p, sql, page, page_size).await,
            DbPool::Postgres(p) => exec_paged_pg(p, sql, page, page_size).await,
        }
    };

    match tokio::time::timeout(Duration::from_millis(timeout_ms.max(1000) as u64), fut).await {
        Ok(Ok(mut r)) => {
            r.exec_ms = dbloom_common::time::now_ms() - start;
            r.success = true;
            Ok(r)
        }
        Ok(Err(e)) => Err(e),
        Err(_) => Err(format!(
            "查询超时（>{}ms，上限 120s）；可选择缩短查询或分页",
            timeout_ms.max(1000)
        )),
    }
}

/// 写语句：执行并返回受影响行数（仅单语句；多语句/危险操作由上层 D6 拦截）。
pub async fn exec_write(
    pool: &DbPool,
    sql: &str,
    timeout_ms: i64,
) -> Result<i64, String> {
    let start = dbloom_common::time::now_ms();
    let fut = async {
        match pool {
            DbPool::MySql(p) => {
                sqlx::query(sql).execute(p).await.map(|r| r.rows_affected() as i64)
            }
            DbPool::Postgres(p) => {
                sqlx::query(sql).execute(p).await.map(|r| r.rows_affected() as i64)
            }
        }
        .map_err(|e| crate::clean_err(&e.to_string()))
    };
    match tokio::time::timeout(Duration::from_millis(timeout_ms.max(1000) as u64), fut).await {
        Ok(r) => r.map(|n| {
            tracing::info!("写语句执行成功，affected={n}（{:.0}ms）", dbloom_common::time::now_ms() - start);
            n
        }),
        Err(_) => Err("写语句执行超时".into()),
    }
}

/// 冒烟：建池时 SELECT 1 由上层 test_connection 负责，此处只做池连通断言（meta 用）。
pub async fn ping(pool: &DbPool) -> Result<(), String> {
    match pool {
        DbPool::MySql(p) => {
            sqlx::query_scalar::<_, i32>("SELECT 1")
                .fetch_one(p)
                .await
                .map(|_| ())
                .map_err(|e| crate::clean_err(&e.to_string()))
        }
        DbPool::Postgres(p) => {
            sqlx::query_scalar::<_, i32>("SELECT 1")
                .fetch_one(p)
                .await
                .map(|_| ())
                .map_err(|e| crate::clean_err(&e.to_string()))
        }
    }
}

// ---------------- 分页执行 MySQL ----------------

async fn exec_paged_mysql(
    pool: &MySqlPool,
    sql: &str,
    page: i64,
    page_size: i64,
) -> Result<QueryResult, String> {
    let mut rows = sqlx::query(sql).fetch(pool);
    use futures::StreamExt;
    let mut qr = QueryResult {
        success: false,
        columns: vec![],
        rows: vec![],
        total: 0,
        page,
        page_size,
        has_more: false,
        exec_ms: 0,
        affected_rows: None,
        need_confirm: None,
        confirmed: None,
    };
    // 流式分页：跳过 [0, skip)，取 [skip, skip+limit)，再取 1 行判 has_more。
    // 不拼 LIMIT/OFFSET，避免破坏用户 SQL 自带 LIMIT 与 SHOW/DESCRIBE 等语句。
    let skip = ((page - 1) * page_size) as usize;
    let limit = page_size as usize;
    let mut idx: usize = 0;
    while let Some(row) = rows.next().await {
        let row = row.map_err(|e| crate::clean_err(&e.to_string()))?;
        if qr.columns.is_empty() {
            qr.columns = row
                .columns()
                .iter()
                .map(|c| ColumnMeta {
                    name: c.name().to_string(),
                    type_name: c.type_info().name().to_string(),
                    nullable: false,
                })
                .collect();
        }
        if idx < skip {
            idx += 1;
            continue;
        }
        if idx - skip >= limit {
            qr.has_more = true;
            break;
        }
        let vals: Vec<serde_json::Value> = (0..row.len()).map(|i| decode_cell_mysql(&row, i)).collect();
        qr.rows.push(vals);
        idx += 1;
    }
    qr.total = qr.rows.len() as i64;
    Ok(qr)
}

async fn exec_paged_pg(
    pool: &PgPool,
    sql: &str,
    page: i64,
    page_size: i64,
) -> Result<QueryResult, String> {
    let mut rows = sqlx::query(sql).fetch(pool);
    use futures::StreamExt;
    let mut qr = QueryResult {
        success: false,
        columns: vec![],
        rows: vec![],
        total: 0,
        page,
        page_size,
        has_more: false,
        exec_ms: 0,
        affected_rows: None,
        need_confirm: None,
        confirmed: None,
    };
    let skip = ((page - 1) * page_size) as usize;
    let limit = page_size as usize;
    let mut idx: usize = 0;
    while let Some(row) = rows.next().await {
        let row = row.map_err(|e| crate::clean_err(&e.to_string()))?;
        if qr.columns.is_empty() {
            qr.columns = row
                .columns()
                .iter()
                .map(|c| ColumnMeta {
                    name: c.name().to_string(),
                    type_name: c.type_info().name().to_string(),
                    nullable: false,
                })
                .collect();
        }
        if idx < skip {
            idx += 1;
            continue;
        }
        if idx - skip >= limit {
            qr.has_more = true;
            break;
        }
        let vals: Vec<serde_json::Value> = (0..row.len()).map(|i| decode_cell_pg(&row, i)).collect();
        qr.rows.push(vals);
        idx += 1;
    }
    qr.total = qr.rows.len() as i64;
    Ok(qr)
}

// ---------------- 单元格解码 ----------------

fn decode_cell_mysql(row: &sqlx::mysql::MySqlRow, i: usize) -> serde_json::Value {
    let raw = match row.try_get_raw(i) {
        Ok(r) => r,
        Err(_) => return serde_json::Value::Null,
    };
    if raw.is_null() {
        return serde_json::Value::Null;
    }
    let t = raw.type_info().name().to_ascii_uppercase();
    let t = t.as_str();

    if t.contains("INT") || t == "BOOL" || t == "BIT" || t == "YEAR" {
        if let Ok(v) = row.try_get::<i32, _>(i) {
            return json!(v);
        }
        if let Ok(v) = row.try_get::<i64, _>(i) {
            return json!(v);
        }
    }
    if t.contains("FLOAT") || t.contains("DOUBLE") || t.contains("DECIMAL") || t.contains("NUMERIC")
        || t.contains("REAL")
    {
        if let Ok(v) = row.try_get::<f64, _>(i) {
            return json!(v);
        }
        if let Ok(v) = row.try_get::<i64, _>(i) {
            return json!(v);
        }
    }
    if t.contains("JSON") {
        if let Ok(v) = row.try_get::<serde_json::Value, _>(i) {
            return v;
        }
    }
    if t.contains("BLOB") || t.contains("BINARY") || t.contains("VARBINARY") {
        if let Ok(v) = row.try_get::<Vec<u8>, _>(i) {
            return json!(format!("<bin:{}B>", v.len()));
        }
    }
    if let Ok(v) = row.try_get::<String, _>(i) {
        return json!(v);
    }
    // 兜底：无法解码的复杂列（时间/几何等）以类型占位表示（不崩溃、不泄露二进制）
    json!(format!("<{t}>"))
}

fn decode_cell_pg(row: &sqlx::postgres::PgRow, i: usize) -> serde_json::Value {
    let raw = match row.try_get_raw(i) {
        Ok(r) => r,
        Err(_) => return serde_json::Value::Null,
    };
    if raw.is_null() {
        return serde_json::Value::Null;
    }
    let t = raw.type_info().name().to_ascii_uppercase();
    let t = t.as_str();

    if t.contains("INT8") || t == "BIGINT" || t.contains("SERIAL8") {
        if let Ok(v) = row.try_get::<i64, _>(i) {
            return json!(v);
        }
        if let Ok(v) = row.try_get::<i32, _>(i) {
            return json!(v);
        }
    }
    if t.contains("INT") || t.contains("SERIAL") || t.contains("SMALLINT") {
        if let Ok(v) = row.try_get::<i32, _>(i) {
            return json!(v);
        }
        if let Ok(v) = row.try_get::<i64, _>(i) {
            return json!(v);
        }
    }
    if t.contains("FLOAT") || t.contains("DOUBLE") || t.contains("DECIMAL") || t.contains("NUMERIC")
        || t.contains("REAL")
    {
        if let Ok(v) = row.try_get::<f64, _>(i) {
            return json!(v);
        }
        if let Ok(v) = row.try_get::<i64, _>(i) {
            return json!(v);
        }
    }
    if t == "BOOL" || t == "BOOLEAN" {
        if let Ok(v) = row.try_get::<bool, _>(i) {
            return json!(v);
        }
    }
    if t.contains("JSON") {
        if let Ok(v) = row.try_get::<serde_json::Value, _>(i) {
            return v;
        }
    }
    if t.contains("BYTEA") {
        if let Ok(v) = row.try_get::<Vec<u8>, _>(i) {
            return json!(format!("<bin:{}B>", v.len()));
        }
    }
    if t.contains("CHAR") || t.contains("TEXT") || t.contains("DATE") || t.contains("TIME")
        || t.contains("UUID") || t.contains("ENUM") || t.contains("MONEY") || t.contains("INTERVAL")
    {
        if let Ok(v) = row.try_get::<String, _>(i) {
            return json!(v);
        }
    }
    if let Ok(v) = row.try_get::<String, _>(i) {
        return json!(v);
    }
    // 兜底：无法解码的复杂列以类型占位表示（不崩溃、不泄露二进制）
    json!(format!("<{t}>"))
}
