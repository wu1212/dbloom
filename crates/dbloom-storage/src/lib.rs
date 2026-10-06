//! dbloom-storage：元数据访问层（ORM 层，D10）。
//!
//! - MySQL 默认、PostgreSQL 可切换（v1.1/D10 拍板，原 SQLite 决策已废弃）；
//! - sqlx 连接池 + 版本化迁移（`migrations/` 目录）；
//! - DAO 按表分组，**租户过滤在数据访问层强制**（`TenantScope`，D8）；
//!   所有业务实体查询注入 `owner_user_id`，例外：sessions/api_keys/audit_logs
//!   属系统/管理员级（`docs/design/01-data-model.md` §3.4）。
//!
//! 依赖方向：`dbloom-types` → `dbloom-storage` → `dbloom-iam`。
//! 本 crate 只做数据访问，不做业务规则。

pub mod error;
pub mod migrate;
pub mod pool;
pub mod tenant;

pub mod dao;

pub use migrate::run_migrations;
pub use pool::connect_pool;
pub use tenant::TenantScope;
