//! 行级数据浏览 / 编辑 / 删除（data，02-api §2.7）。
//!
//! - 表名/列名一律 `valid_ident`（仅 `[A-Za-z_][A-Za-z0-9_]*`）防注入；值全部走绑定参数；
//! - DELETE/UPDATE 由上层（server）过 D6 写保护（只读锁 / confirm）；本层只负责执行；
//! - 始终按主键/指定的 WHERE 键匹配单行（`keys` 非空校验）；不提供无条件的行删除。

use crate::query::{self, DbPool};
use dbloom_common::AppError;
use dbloom_types::RowKey;
use sqlx::Row as _;

/// 行浏览：返回列 + 当前页行 + 总行数。
pub async fn list_rows(
    pool: &DbPool,
    table: &str,
    page: i64,
    page_size: i64,
) -> Result<(Vec<dbloom_types::ColumnMeta>, Vec<Vec<serde_json::Value>>, i64), String> {
    let table = valid_ident(table)?;
    let sql = format!("SELECT * FROM {table}");
    let mut r = query::query_rows(pool, &sql, page, page_size, query::DEFAULT_TIMEOUT_MS).await?;
    let total: i64 = match pool {
        DbPool::MySql(p) => {
            let c: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
                .fetch_one(p)
                .await
                .map_err(|e| crate::clean_err(&e.to_string()))?;
            c
        }
        DbPool::Postgres(p) => {
            let c: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
                .fetch_one(p)
                .await
                .map_err(|e| crate::clean_err(&e.to_string()))?;
            c
        }
    };
    let cols = std::mem::take(&mut r.columns);
    let rows = std::mem::take(&mut r.rows);
    Ok((cols, rows, total))
}

/// 删除一行：`DELETE FROM t WHERE k1=? AND k2=?`（值为绑定参数）。
pub async fn delete_row(pool: &DbPool, table: &str, keys: &[RowKey]) -> Result<i64, String> {
    if keys.is_empty() {
        return Err(AppError::validation("删除行必须提供主键/匹配键（keys 不能为空）").message);
    }
    let table = valid_ident(table)?;
    match pool {
        DbPool::MySql(p) => {
            let (where_sql, _) = build_where(keys)?;
            let sql = format!("DELETE FROM {table} {where_sql}");
            let mut q = sqlx::query(&sql);
            for k in keys {
                q = bind_val_mysql(q, &k.value);
            }
            let n = q
                .execute(p)
                .await
                .map_err(|e| crate::clean_err(&e.to_string()))?
                .rows_affected();
            Ok(n as i64)
        }
        DbPool::Postgres(p) => {
            let (where_sql, _) = build_where_pg(keys, 0)?;
            let sql = format!("DELETE FROM {table} {where_sql}");
            let mut q = sqlx::query(&sql);
            for k in keys {
                q = bind_val_pg(q, &k.value);
            }
            let n = q
                .execute(p)
                .await
                .map_err(|e| crate::clean_err(&e.to_string()))?
                .rows_affected();
            Ok(n as i64)
        }
    }
}

/// 更新一行：`UPDATE t SET v1=? WHERE k1=? AND k2=?`。
pub async fn update_row(
    pool: &DbPool,
    table: &str,
    keys: &[RowKey],
    values: &[RowKey],
) -> Result<i64, String> {
    if keys.is_empty() {
        return Err(AppError::validation("更新行必须提供主键/匹配键（keys 不能为空）").message);
    }
    if values.is_empty() {
        return Err(AppError::validation("更新行必须提供要修改的字段（values 不能为空）").message);
    }
    let table = valid_ident(table)?;
    for k in values.iter().chain(keys) {
        valid_ident(&k.column)?;
    }
    let set_cols: Vec<&str> = values.iter().map(|k| k.column.as_str()).collect();
    let (where_sql, _) = build_where(keys)?;
    let set_clause = set_cols
        .iter()
        .map(|c| format!("{c} = ?"))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!("UPDATE {table} SET {set_clause} {where_sql}");
    match pool {
        DbPool::MySql(p) => {
            let mut q = sqlx::query(&sql);
            for v in values {
                q = bind_val_mysql(q, &v.value);
            }
            for k in keys {
                q = bind_val_mysql(q, &k.value);
            }
            let n = q
                .execute(p)
                .await
                .map_err(|e| crate::clean_err(&e.to_string()))?
                .rows_affected();
            Ok(n as i64)
        }
        DbPool::Postgres(p) => {
            let set_cols: Vec<&str> = values.iter().map(|k| k.column.as_str()).collect();
            let set_clause = set_cols
                .iter()
                .enumerate()
                .map(|(i, c)| format!("{c} = ${}", i + 1))
                .collect::<Vec<_>>()
                .join(", ");
            let (where_sql, w_offs) = build_where_pg(keys, values.len())?;
            let sql = format!("UPDATE {table} SET {set_clause} {where_sql}");
            let _ = w_offs;
            let mut q = sqlx::query(&sql);
            for v in values {
                q = bind_val_pg(q, &v.value);
            }
            for k in keys {
                q = bind_val_pg(q, &k.value);
            }
            let n = q
                .execute(p)
                .await
                .map_err(|e| crate::clean_err(&e.to_string()))?
                .rows_affected();
            Ok(n as i64)
        }
    }
}

/// 生成 WHERE 子句（mysql `?` 占位）。
fn build_where(
    keys: &[RowKey],
) -> Result<(String, ()), String> {
    let conds: Vec<String> = keys
        .iter()
        .map(|k| format!("{} = ?", k.column))
        .collect();
    Ok((
        format!("WHERE {}",
            if conds.is_empty() { "1 <> 1".to_string() } else { conds.join(" AND ") },
        ), (),
    ))
}

/// 生成 WHERE 子句（postgres `$n` 占位，从 offset 起编号）。
fn build_where_pg(keys: &[RowKey], offset: usize) -> Result<(String, usize), String> {
    let conds: Vec<String> = keys
        .iter()
        .enumerate()
        .map(|(i, k)| format!("{} = ${}", k.column, offset + i + 1))
        .collect();
    Ok((
        format!("WHERE {}",
            if conds.is_empty() { "1 <> 1".to_string() } else { conds.join(" AND ") },
        ), offset + keys.len(),
    ))
}

/// 按 JSON 类型绑定值（MySQL：宽松隐式转换；空串/对象兜底字符化）。
fn bind_val_mysql<'q>(
    q: sqlx::query::Query<'q, sqlx::MySql, sqlx::mysql::MySqlArguments>,
    v: &'q serde_json::Value,
) -> sqlx::query::Query<'q, sqlx::MySql, sqlx::mysql::MySqlArguments> {
    match v {
        serde_json::Value::Null => q.bind(Option::<String>::None),
        serde_json::Value::Bool(b) => q.bind(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                q.bind(i)
            } else if let Some(f) = n.as_f64() {
                q.bind(f)
            } else {
                q.bind(n.to_string())
            }
        }
        serde_json::Value::String(s) => q.bind(s.as_str()),
        other => q.bind(serde_json::to_string(other).unwrap_or_default()),
    }
}

/// 按 JSON 类型绑定值（PostgreSQL：严格要求类型匹配，前端应提交数值类型）。
fn bind_val_pg<'q>(
    q: sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments>,
    v: &'q serde_json::Value,
) -> sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments> {
    match v {
        serde_json::Value::Null => q.bind(Option::<String>::None),
        serde_json::Value::Bool(b) => q.bind(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                q.bind(i)
            } else if let Some(f) = n.as_f64() {
                q.bind(f)
            } else {
                q.bind(n.to_string())
            }
        }
        serde_json::Value::String(s) => q.bind(s.as_str()),
        other => q.bind(serde_json::to_string(other).unwrap_or_default()),
    }
}

/// 标识符合法性（防 SQL 注入）：`[A-Za-z_][A-Za-z0-9_]*`。
fn valid_ident(s: &str) -> Result<&str, String> {
    let b = s.as_bytes();
    if b.is_empty()
        || !(b[0].is_ascii_alphabetic() || b[0] == b'_')
        || !b.iter().all(|c| c.is_ascii_alphanumeric() || *c == b'_')
    {
        Err(format!("非法标识符（仅字母/数字/下划线）: {s}"))
    } else {
        Ok(s)
    }
}

/// 供测试引用（避免 unused 警告）。
#[allow(dead_code)]
fn _row_len(r: &sqlx::mysql::MySqlRow) -> usize {
    r.len()
}
