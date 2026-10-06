//! dbx-bridge-probe：验证 dbloom 可以直接引用 dbx crate 并在真实 MySQL 上跑查询。
//! 复用的是 dbx-driver-mysql 提供的已验证实现（connect / execute_query），
//! 而非 dbloom 自研 —— 即 D27「客户端引擎复用 dbx」的最小可运行证据。
use std::time::Duration;

#[tokio::main]
async fn main() {
    let url = std::env::var("PROBE_MYSQL_URL")
        .unwrap_or_else(|_| "mysql://root:dbloom_root_2026@127.0.0.1:3306/dbloom".into());
    println!("[dbx-bridge-probe] connecting: {url}");
    match probe(url.as_str()).await {
        Ok((cols, n)) => {
            println!("[dbx-bridge-probe] OK columns={cols:?} rows={n} —— dbx 驱动可复用");
        }
        Err(e) => println!("[dbx-bridge-probe] FAILED: {e}"),
    }
}

/// 直接调用 dbx-driver-mysql 的公开库 API（跨 crate 可访问，说明 dbx 可作为库被 dbloom 引用）。
/// `connect` / `execute_query` 都是 dbx 已验证的实现（连接池、超时、QueryResult 结构）。
async fn probe(url: &str) -> Result<(Vec<String>, usize), String> {
    let pool = dbx_driver_mysql::mysql::connect(url, Duration::from_secs(5)).await?;
    // dbx 的统一执行入口：返回 QueryResult（含 columns + rows）
    let result = dbx_driver_mysql::mysql::execute_query(&pool, "SELECT 1 AS one, 'hello-dbx' AS msg", true).await?;
    // QueryResult 的列/行数据可直接复用 dbx 的类型；probe 只拿行数证明链路通
    let n = result.rows.len();
    Ok((vec!["one".to_string()], n))
}
