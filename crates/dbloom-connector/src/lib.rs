//! dbloom-connector：数据库客户端引擎（D17 统一转发执行）。
//!
//! M0 为空壳；M1（连接管理/连接测试）与 M2（SQL 工作台/元数据/行编辑/导出导入）
//! 在本 crate 落地：连接池、`DbDriver` trait 适配层、写保护（D6）、导出（D23）。
//! 首批 6 库：mysql/postgres/sqlserver/mongodb/redis/elasticsearch（D7）。

pub fn placeholder() -> &'static str {
    "dbloom-connector: M1 起实现（数据库客户端引擎）"
}
