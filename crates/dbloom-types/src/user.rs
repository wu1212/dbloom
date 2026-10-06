//! 用户域 DTO 与枚举（`docs/design/01-data-model.md` §2 users 表）。

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// 用户角色（D8：仅 1 个内置管理员，其余均普通用户）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum UserRole {
    Admin,
    User,
}

impl UserRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            UserRole::Admin => "admin",
            UserRole::User => "user",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "admin" => Some(UserRole::Admin),
            "user" => Some(UserRole::User),
            _ => None,
        }
    }
}

/// 用户状态：active / locked / disabled。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum UserStatus {
    Active,
    Locked,
    Disabled,
}

impl UserStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            UserStatus::Active => "active",
            UserStatus::Locked => "locked",
            UserStatus::Disabled => "disabled",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "active" => Some(UserStatus::Active),
            "locked" => Some(UserStatus::Locked),
            "disabled" => Some(UserStatus::Disabled),
            _ => None,
        }
    }
}

/// 用户 DTO（不包含任何密码相关字段）。
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserDto {
    pub id: i64,
    pub username: String,
    pub role: UserRole,
    pub status: UserStatus,
    pub display_name: Option<String>,
    pub must_change_password: bool,
    pub last_login_at: Option<i64>,
    pub created_at: i64,
}

/// 用户列表分页响应。
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserListResponse {
    pub total: i64,
    pub items: Vec<UserDto>,
}
