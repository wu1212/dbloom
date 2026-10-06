//! 连接池：从 `DB_DSN`（如 `mysql://user:pass@host:3306/dbloom`）创建 sqlx 连接池。
//!
//! 未提供 `DB_DSN` 时，默认尝试本地 `mysql://root@localhost:3306/dbloom`
//! （便于本地开发初启；生产必须显式配置环境变量）。

use sqlx::mysql::{MySqlConnectOptions, MySqlPool, MySqlPoolOptions};
use std::str::FromStr;

/// MySQL 连接池工厂。
pub async fn connect_pool(dsn: Option<&str>) -> Result<MySqlPool, sqlx::Error> {
    let opts = match dsn {
        Some(d) => MySqlConnectOptions::from_str(d).map_err(|e| sqlx::Error::Protocol(e.to_string().into()))?,
        None => MySqlConnectOptions::from_str("mysql://root@localhost:3306/dbloom").unwrap(),
    };

    MySqlPoolOptions::new()
        .max_connections(10)
        .min_connections(1)
        .acquire_timeout(std::time::Duration::from_secs(5))
        .connect_with(opts)
        .await
}
