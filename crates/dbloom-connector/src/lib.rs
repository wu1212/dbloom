//! dbloom-connector：数据库客户端引擎（D17 统一转发执行）。
//!
//! M1：驱动适配层 + 连接测试（`driver`）；连接池管理与 SQL 能力（query/schema/
//! rows/edit/export/import）在 M2 落地；首批 6 库：mysql/postgres/sqlserver/
//! mongodb/redis/elasticsearch（D7，`dbloom-types::manifest` 统一声明）。

pub mod driver;

pub use driver::{ConnParams, TestOutcome, test_connection};

