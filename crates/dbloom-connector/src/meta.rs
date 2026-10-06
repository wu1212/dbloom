//! 元数据读取（schema）：databases / tables / columns / DDL。
//!
//! 统一基于 `information_schema`（mysql/postgres 兼容子集）+ 主键 key_column_usage；
//! DDL：mysql 用 `SHOW CREATE TABLE`，postgres 用 information_schema 生成可读 DDL。

use crate::query::DbPool;
use dbloom_types::{ColumnItem, TableItem};
use sqlx::Row as _;

/// 列出可用库（排除系统库）。
pub async fn databases(pool: &DbPool) -> Result<Vec<String>, String> {
    let sql = "SELECT schema_name FROM information_schema.schemata \
               WHERE schema_name NOT IN \
               ('information_schema','performance_schema','mysql','sys','pg_catalog','pg_toast','pg_temp_1','pg_toast_temp_1','template0','template1') \
               ORDER BY schema_name";
    let names = match pool {
        DbPool::MySql(p) => {
            let rows = sqlx::query(sql).fetch_all(p).await.map_err(|e| crate::clean_err(&e.to_string()))?;
            rows.iter().map(|r| r.get::<String, _>(0)).collect::<Vec<_>>()
        }
        DbPool::Postgres(p) => {
            let rows = sqlx::query(sql).fetch_all(p).await.map_err(|e| crate::clean_err(&e.to_string()))?;
            rows.iter().map(|r| r.get::<String, _>(0)).collect::<Vec<_>>()
        }
    };
    Ok(names)
}

/// 列出某库下的表/视图。
pub async fn tables(pool: &DbPool, db: &str) -> Result<Vec<TableItem>, String> {
    match pool {
        DbPool::MySql(p) => {
            let rows = sqlx::query(
                "SELECT table_name, table_type, table_comment \
                 FROM information_schema.tables WHERE table_schema=? ORDER BY table_name",
            )
            .bind(db)
            .fetch_all(p)
            .await
            .map_err(|e| crate::clean_err(&e.to_string()))?;
            Ok(rows
                .iter()
                .map(|r| TableItem {
                    name: r.get::<String, _>(0),
                    table_type: r.get::<String, _>(1),
                    comment: r.try_get::<String, _>(2).ok().filter(|s| !s.is_empty()),
                })
                .collect())
        }
        DbPool::Postgres(p) => {
            let rows = sqlx::query(
                "SELECT table_name, table_type FROM information_schema.tables \
                 WHERE table_schema=$1 ORDER BY table_name",
            )
            .bind(db)
            .fetch_all(p)
            .await
            .map_err(|e| crate::clean_err(&e.to_string()))?;
            Ok(rows
                .iter()
                .map(|r| TableItem {
                    name: r.get::<String, _>(0),
                    table_type: r.get::<String, _>(1),
                    comment: None,
                })
                .collect())
        }
    }
}

/// 列出某表列（含主键标记）。
pub async fn columns(pool: &DbPool, db: &str, table: &str) -> Result<Vec<ColumnItem>, String> {
    match pool {
        DbPool::MySql(p) => {
            let rows = sqlx::query(
                "SELECT c.column_name, c.data_type, c.is_nullable, c.column_default, c.column_comment, \
                        k.column_name IS NOT NULL AS is_primary \
                 FROM information_schema.columns c \
                 LEFT JOIN ( \
                     SELECT kcu.table_schema, kcu.table_name, kcu.column_name \
                     FROM information_schema.key_column_usage kcu \
                     JOIN information_schema.table_constraints tc \
                       ON tc.constraint_name=kcu.constraint_name AND tc.constraint_schema=kcu.constraint_schema \
                     WHERE tc.constraint_type='PRIMARY KEY' \
                 ) k ON k.table_schema=c.table_schema AND k.table_name=c.table_name AND k.column_name=c.column_name \
                 WHERE c.table_schema=? AND c.table_name=? ORDER BY c.ordinal_position",
            )
            .bind(db)
            .bind(table)
            .fetch_all(p)
            .await
            .map_err(|e| crate::clean_err(&e.to_string()))?;
            Ok(rows
                .iter()
                .map(|r| ColumnItem {
                    name: r.get::<String, _>(0),
                    data_type: r.get::<String, _>(1),
                    nullable: r.get::<String, _>(2).eq_ignore_ascii_case("YES"),
                    default_value: r.try_get::<String, _>(3).ok().map(serde_json::Value::String),
                    comment: r.try_get::<String, _>(4).ok().filter(|s| !s.is_empty()),
                    is_primary: r.try_get::<i32, _>(5).map(|v| v != 0).unwrap_or(false),
                })
                .collect())
        }
        DbPool::Postgres(p) => {
            let rows = sqlx::query(
                "SELECT c.column_name, c.data_type, c.is_nullable, c.column_default, \
                        pg_catalog.col_description(format('%I.%I', c.table_schema, c.table_name)::regclass::oid, c.ordinal_position) AS column_comment, \
                        k.column_name IS NOT NULL AS is_primary \
                 FROM information_schema.columns c \
                 LEFT JOIN ( \
                     SELECT kcu.table_schema, kcu.table_name, kcu.column_name \
                     FROM information_schema.key_column_usage kcu \
                     JOIN information_schema.table_constraints tc \
                       ON tc.constraint_name=kcu.constraint_name AND tc.constraint_schema=kcu.constraint_schema \
                     WHERE tc.constraint_type='PRIMARY KEY' \
                 ) k ON k.table_schema=c.table_schema AND k.table_name=c.table_name AND k.column_name=c.column_name \
                 WHERE c.table_schema=$1 AND c.table_name=$2 ORDER BY c.ordinal_position",
            )
            .bind(db)
            .bind(table)
            .fetch_all(p)
            .await
            .map_err(|e| crate::clean_err(&e.to_string()))?;
            Ok(rows
                .iter()
                .map(|r| ColumnItem {
                    name: r.get::<String, _>(0),
                    data_type: r.get::<String, _>(1),
                    nullable: r.get::<String, _>(2).eq_ignore_ascii_case("YES"),
                    default_value: r.try_get::<String, _>(3).ok().map(serde_json::Value::String),
                    comment: r.try_get::<String, _>(4).ok().filter(|s| !s.is_empty()),
                    is_primary: r.try_get::<bool, _>(5).unwrap_or(false),
                })
                .collect())
        }
    }
}

/// DDL：mysql `SHOW CREATE TABLE`；pg 用 information_schema 拼可读 DDL（非 pg_dump 原生）。
pub async fn table_ddl(pool: &DbPool, db: &str, table: &str) -> Result<String, String> {
    match pool {
        DbPool::MySql(p) => {
            let q = format!("SHOW CREATE TABLE `{}`.`{}`", quote_ident_mysql(db), quote_ident_mysql(table));
            let row = sqlx::query(&q)
                .fetch_one(p)
                .await
                .map_err(|e| crate::clean_err(&e.to_string()))?;
            let ddl = row.try_get::<String, _>(1).unwrap_or_else(|_| row.try_get::<String, _>(0).unwrap_or_default());
            Ok(ddl)
        }
        DbPool::Postgres(p) => {
            let cols = sqlx::query(
                "SELECT column_name, data_type, is_nullable FROM information_schema.columns \
                 WHERE table_schema=$1 AND table_name=$2 ORDER BY ordinal_position",
            )
            .bind(db)
            .bind(table)
            .fetch_all(p)
            .await
            .map_err(|e| crate::clean_err(&e.to_string()))?;
            let pk = sqlx::query(
                "SELECT kcu.column_name FROM information_schema.key_column_usage kcu \
                 JOIN information_schema.table_constraints tc \
                   ON tc.constraint_name=kcu.constraint_name AND tc.constraint_schema=kcu.constraint_schema \
                 WHERE tc.constraint_type='PRIMARY KEY' AND tc.table_schema=$1 AND tc.table_name=$2 \
                 ORDER BY kcu.ordinal_position",
            )
            .bind(db)
            .bind(table)
            .fetch_all(p)
            .await
            .map_err(|e| crate::clean_err(&e.to_string()))?;
            let mut lines: Vec<String> = cols
                .iter()
                .map(|r| {
                    let c = r.get::<String, _>(0);
                    let t = r.get::<String, _>(1);
                    let nn = r.get::<String, _>(2).eq_ignore_ascii_case("NO");
                    format!("    {c} {t}{}", if nn { " NOT NULL" } else { "" })
                })
                .collect();
            if !pk.is_empty() {
                let cols: Vec<String> = pk.iter().map(|r| r.get::<String, _>(0)).collect();
                lines.push(format!("    PRIMARY KEY ({})", cols.join(", ")));
            }
            Ok(format!(
                "CREATE TABLE {}.{} (\n{}\n);",
                quote_ident_pg(db),
                quote_ident_pg(table),
                lines.join(",\n")
            ))
        }
    }
}

fn quote_ident_mysql(s: &str) -> String {
    s.replace('`', "``")
}
fn quote_ident_pg(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}
