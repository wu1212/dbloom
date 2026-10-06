//! API Key 域 DTO（`docs/design/02-api.md` §2.3；权限矩阵 admin-only，D9）。

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// API Key 状态：enabled / disabled。时间窗由 valid_from / valid_until 控制。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum ApiKeyStatus {
    Enabled,
    Disabled,
}

impl ApiKeyStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ApiKeyStatus::Enabled => "enabled",
            ApiKeyStatus::Disabled => "disabled",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "enabled" => Some(ApiKeyStatus::Enabled),
            "disabled" => Some(ApiKeyStatus::Disabled),
            _ => None,
        }
    }
}

/// API Key 摘要（永不回传明文密钥；明文仅在签发的 CreateApiKeyResponse.secret 出现一次）。
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ApiKeyDto {
    pub id: i64,
    pub user_id: i64,
    pub user_username: String,
    pub name: String,
    /// 展示前缀（如 `dbk_a1b2c3…`），便于人工识别。
    pub prefix: String,
    pub status: ApiKeyStatus,
    pub valid_from: Option<i64>,
    pub valid_until: Option<i64>,
    pub last_used_at: Option<i64>,
    pub created_at: i64,
}

/// 签发 Key 请求（HTTP 体直接用 user_id 指明归属；或走 path）。
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateApiKeyRequest {
    pub name: String,
    #[serde(default)]
    pub valid_from: Option<i64>,
    #[serde(default)]
    pub valid_until: Option<i64>,
}

/// 签发响应：`secret` 为明文，**仅此一次返回**。
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateApiKeyResponse {
    pub key: ApiKeyDto,
    pub secret: String,
}

/// 更新 Key（name / status / 生效 / 失效时间）。
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateApiKeyRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub status: Option<ApiKeyStatus>,
    #[serde(default)]
    pub valid_from: Option<i64>,
    #[serde(default)]
    pub valid_until: Option<i64>,
}
