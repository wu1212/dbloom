//! dbloom-server：HTTP(S)+WS 装配、路由、OpenAPI 契约（二进制）。
//!
//! 依赖方向：`dbloom-iam` → `dbloom-server`。
//! M0 路由：`auth/*`、`users/*`、`api-keys/*`、`health`（`02-api.md` §2.1–2.3）。

pub mod api;
pub mod auth;
pub mod error;
pub mod openapi;
pub mod state;

pub use error::ApiError;
pub use state::AppState;
