//! 连接（connections）DTO（`docs/design/02-api.md` §2.4）。
//!
//! 安全约定：**任何响应不回传密码**（`password_enc` 永不出现在 DTO），
//! 明文密码只存在于请求体（创建/更新）与服务端内存。

use serde::{Deserialize, Serialize};

/// 连接详情/列表统一模型（脱敏：无 password 字段）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionDto {
    pub id: i64,
    pub owner_user_id: i64,
    pub name: String,
    pub conn_type: String,
    pub host: String,
    pub port: Option<i32>,
    pub database_name: Option<String>,
    pub username: Option<String>,
    pub ssl_mode: String,
    pub extra_params: Option<serde_json::Value>,
    pub is_production: bool,
    pub read_only_lock: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

/// 连接列表响应（分页）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionListResponse {
    pub total: i64,
    pub items: Vec<ConnectionDto>,
}

/// 创建连接请求。
#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateConnectionRequest {
    pub name: String,
    pub conn_type: String,
    pub host: String,
    pub port: Option<i32>,
    pub database_name: Option<String>,
    pub username: Option<String>,
    /// 明文密码；创建必传（可空字符串=无密码）。
    pub password: Option<String>,
    pub ssl_mode: Option<String>,
    pub extra_params: Option<serde_json::Value>,
    pub is_production: Option<bool>,
    pub read_only_lock: Option<bool>,
}

/// 更新连接请求：`password` 为 None 表示不修改密码；其余字段 None 表示保持原值。
#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateConnectionRequest {
    pub name: Option<String>,
    pub host: Option<String>,
    pub port: Option<i32>,
    pub database_name: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub ssl_mode: Option<String>,
    pub extra_params: Option<serde_json::Value>,
    pub is_production: Option<bool>,
    pub read_only_lock: Option<bool>,
}

/// 连接测试结果（业务结果，200 返回；D17 server 端发起）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestConnectionResponse {
    pub success: bool,
    pub message: String,
    pub latency_ms: i64,
}
