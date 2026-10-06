//! DAO：按表分组的数据库访问方法。
//!
//! 规则（`docs/design/01-data-model.md` §3.4）：
//! - `connections/tasks/task_runs/alert_rules/files` 查询强制租户过滤（M1+ 落地）；
//! - `sessions/api_keys/audit_logs` 属系统/管理员级资源（本 M0 涉及的表即此三类 + users）。
//!   `api_keys.user_id` 是绑定关系（由管理员管理），不属于普通用户的私有租户数据。

mod api_keys;
mod audit;
mod connections;
mod sessions;
mod users;

pub use api_keys::{ApiKeyDao, ApiKeyRow};
pub use audit::{AuditDao, AuditRow};
pub use connections::{ConnectionDao, ConnectionRow, ConnectionUpdate};
pub use sessions::{SessionDao, SessionRow};
pub use users::{UserDao, UserRow};

pub const MAX_PAGE_SIZE: i64 = 100;
pub const DEFAULT_PAGE_SIZE: i64 = 20;

/// 归一化分页参数：page>=1，page_size 1..=100。
pub fn normalize_page(page: Option<i64>, page_size: Option<i64>) -> (i64, i64) {
    let page = page.unwrap_or(1).max(1);
    let size = page_size.unwrap_or(DEFAULT_PAGE_SIZE).clamp(1, MAX_PAGE_SIZE);
    (page, size)
}
