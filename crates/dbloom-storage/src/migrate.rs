//! 版本化迁移：sqlx migrate（`migrations/*.sql` 编译期嵌入）。
//!
//! 多副本并发迁移防锁：MySQL 由 sqlx migrate 事务化执行 + 版本记录
//! （外部库天然解决多写冲突，D10）。

use dbloom_common::{AppError, Result};
use sqlx::MySqlPool;

/// 全部迁移文件（编译期嵌入 `crates/dbloom-storage/migrations/`）。
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// 应用未执行的迁移（幂等）。
pub async fn run_migrations(pool: &MySqlPool) -> Result<()> {
    MIGRATOR
        .run(pool)
        .await
        .map_err(|e| AppError::internal(format!("数据库迁移失败: {e}")))
}
