//! dbloom-connector：数据库客户端引擎（D17 统一转发执行）。
//!
//! - M1：驱动适配层 + 连接测试（`driver`）；
//! - M2：查询执行（`query`）、元数据（`meta`）、行编辑（`data`）、写保护守卫（`guard`）；
//! - 首批 6 库：mysql/postgres/sqlserver/mongodb/redis/elasticsearch（D7，manifest 统一声明）。
//!
//! 安全：凭据由 server 层解密后以 `ConnParams` 传入，仅存进程内存；
//! 危险/写操作必须先过 `guard::classify_sql` 的 D6 判定（只读锁与 confirm 链路上层执行）。

pub mod data;
pub mod driver;
pub mod guard;
pub mod meta;
pub mod query;

pub use data::{delete_row, list_rows, update_row};
pub use driver::{ConnParams, TestOutcome, test_connection};
pub(crate) use driver::clean_err;
pub use guard::{SqlKind, classify_sql};
pub use meta::{columns, databases, table_ddl, tables};
pub use query::{DbPool, exec_write, open_pool, query_rows, DEFAULT_TIMEOUT_MS, MAX_PAGE_SIZE};
