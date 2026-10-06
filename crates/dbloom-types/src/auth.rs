//! 认证域 DTO（`docs/design/02-api.md` §2.1 Auth）。

use super::user::UserDto;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// 登录请求（匿名）。
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

/// 登录响应：access JWT（2h）+ refresh token（7d）+ 用户信息。
/// 前端存内存 + refresh 落 HttpOnly cookie / localStorage（`04-security.md` §2）。
/// 统一 camelCase（与全部 /api/v1 DTO 一致，前端 OpenAPI 生成对齐）。
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LoginResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub user: UserDto,
}

/// 刷新 access token 请求。
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RefreshRequest {
    pub refresh_token: String,
}

/// 刷新响应（仅新的 access token）。
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RefreshResponse {
    pub access_token: String,
}

/// 修改/强制重置自己密码请求（D20：must_change_password=1 时强制）。
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ChangePasswordRequest {
    pub old_password: String,
    pub new_password: String,
}
