//! dbloom-iam：身份与访问管理（用户/认证/API Key/多租户过滤/审计）。
//!
//! - 密码哈希 argon2（D19）；JWT access 2h + refresh 7d（可吊销，sessions 表）；
//! - 登录失败 5 次/5min 锁定（D19）；首登/被重置强制改密（D20）；
//! - API Key（D9）：仅管理员签发，哈希存储、明文仅一次返回，生效/失效时间窗 +
//!   status 开关；撤销立即全局生效；
//! - 审计写入（D21）。
//!
//! 依赖方向：`dbloom-storage` → `dbloom-iam` → `dbloom-server`。
//! 本 crate 持有业务规则，不感知 HTTP。

pub mod apikey;
pub mod audit;
pub mod iam;
pub mod jwt;
pub mod password;
pub mod session;
pub mod users;

pub use iam::{AuthResult, Config, Iam};
