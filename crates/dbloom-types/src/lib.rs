//! dbloom-types：共享 DTO / 枚举 / 连接类型 manifest / 任务模型。
//!
//! 仅依赖 `dbloom-common`，被 `dbloom-storage`/`dbloom-iam`/`dbloom-server`
//! 与前端 OpenAPI 生成共享。DTO 均带 `utoipa::ToSchema` 以支持 OpenAPI 契约。
//!
//! 连接类型 manifest 见 `docs/design/03-modules.md` §3（M1 落地）。

pub mod auth;
pub mod user;
pub mod apikey;

pub use user::{UserDto, UserListResponse, UserRole, UserStatus};
pub use auth::{LoginRequest, LoginResponse, RefreshRequest, RefreshResponse, ChangePasswordRequest};
pub use apikey::{
    ApiKeyDto, ApiKeyStatus, CreateApiKeyRequest, CreateApiKeyResponse, UpdateApiKeyRequest,
};
