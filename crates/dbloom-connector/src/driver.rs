//! 连接测试（D17：**服务端发起**，前端永不直连数据源）。
//!
//! M1 落地：驱动适配层入口 —— 按连接类型分派到各库客户端，执行「建连 + ping」。
//! 首批实现 mysql/postgres（`dbloom-types::manifest` 的 `implemented=true`）；
//! sqlserver/mongodb/redis/elasticsearch 在 M2 按 manifest 点亮后补齐。
//!
//! `ConnParams` 由 server 层从已保存连接构造（密码已解密，仅存内存）。

use dbloom_common::{Result, time::now_ms};
use sqlx::mysql::{MySqlConnectOptions, MySqlPoolOptions, MySqlSslMode};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions, PgSslMode};

/// 连接测试参数（进程内存中短暂存在，不落盘、不返前端）。
#[derive(Debug, Clone)]
pub struct ConnParams {
    pub conn_type: String,
    pub host: String,
    pub port: Option<u16>,
    pub database: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub ssl_mode: String,
}

/// 连接测试业务结果（成功/失败都是 200 返回的业务数据，不是异常）。
#[derive(Debug, Clone)]
pub enum TestOutcome {
    Success { latency_ms: i64 },
    Failure { message: String },
}

impl TestOutcome {
    pub fn to_dto(&self) -> dbloom_types::TestConnectionResponse {
        match self {
            TestOutcome::Success { latency_ms } => dbloom_types::TestConnectionResponse {
                success: true,
                message: "连接成功".into(),
                latency_ms: *latency_ms,
            },
            TestOutcome::Failure { message } => dbloom_types::TestConnectionResponse {
                success: false,
                message: message.clone(),
                latency_ms: -1,
            },
        }
    }
}

/// 建连 + ping（SELECT 1）。`conn_type` 未知 / 未实现 → Failure（非 AppError）。
pub async fn test_connection(p: &ConnParams) -> Result<TestOutcome> {
    let start = now_ms();
    let r = match p.conn_type.as_str() {
        "mysql" => test_mysql(p).await,
        "postgres" => test_postgres(p).await,
        other => {
            return Ok(TestOutcome::Failure {
                message: format!(
                    "连接类型 {other} 当前版本暂未实现测试（M2 起提供，见 manifest.implemented）"
                ),
            });
        }
    };
    match r {
        Ok(()) => Ok(TestOutcome::Success {
            latency_ms: now_ms() - start,
        }),
        Err(e) => Ok(TestOutcome::Failure { message: e }),
    }
}

async fn test_mysql(p: &ConnParams) -> std::result::Result<(), String> {
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
        .max_connections(1)
        .acquire_timeout(std::time::Duration::from_secs(5))
        .connect_with(opts)
        .await
        .map_err(|e| clean_err(&e.to_string()))?;
    sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(&pool)
        .await
        .map_err(|e| clean_err(&e.to_string()))?;
    pool.close().await;
    Ok(())
}

async fn test_postgres(p: &ConnParams) -> std::result::Result<(), String> {
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
        .max_connections(1)
        .acquire_timeout(std::time::Duration::from_secs(5))
        .connect_with(opts)
        .await
        .map_err(|e| clean_err(&e.to_string()))?;
    sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(&pool)
        .await
        .map_err(|e| clean_err(&e.to_string()))?;
    pool.close().await;
    Ok(())
}

/// 精简驱动错误：换行压缩、截断（防把内部 DSN/上下文喷到前端）。
pub(crate) fn clean_err(e: &str) -> String {
    let one_line = e.replace('\n', " ").trim().to_string();
    let mut s = one_line;
    if s.len() > 300 {
        s.truncate(300);
        s.push_str("…");
    }
    s
}
