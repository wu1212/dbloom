//! 同步任务 DTO（`docs/design/02-api.md` §2.6，D4/D5）。
//!
//! 安全约定：`config_hocon` 快照为**脱敏**后的 HOCON（不含密码，T1 用例 2）。

use serde::{Deserialize, Serialize};

/// 表映射（创建任务请求）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableMappingRequest {
    pub source_table: String, // 源库表（可为 database.table 或仅 table）
    pub sink_table: String,   // 目标库表（可为 database.table 或仅 table）
}

/// 创建任务请求。
#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateTaskRequest {
    pub name: String,
    pub description: Option<String>,
    pub source_connection_id: i64,
    pub sink_connection_id: i64,
    /// batch|increment|cdc（T1 仅 batch 可用）。
    #[serde(default)]
    pub sync_mode: String,
    #[serde(default)]
    pub table_mapping: Vec<TableMappingRequest>,
    pub schedule_cron: Option<String>,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub timeout_sec: i32,
    #[serde(default)]
    pub retry_times: i32,
}

/// 更新任务请求（None=保持原值）。
#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateTaskRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub sync_mode: Option<String>,
    pub schedule_cron: Option<String>,
    pub enabled: Option<bool>,
    pub timeout_sec: Option<i32>,
    pub retry_times: Option<i32>,
}

/// 任务 DTO。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskDto {
    pub id: i64,
    pub owner_user_id: i64,
    pub name: String,
    pub description: Option<String>,
    pub source_connection_id: i64,
    pub sink_connection_id: i64,
    pub sync_mode: String,
    /// 脱敏 HOCON 快照（不含密码）。
    pub config_hocon: String,
    pub schedule_cron: Option<String>,
    pub enabled: bool,
    pub timeout_sec: i32,
    pub retry_times: i32,
    pub created_at: i64,
    pub updated_at: i64,
}

/// 任务列表响应。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskListResponse {
    pub total: i64,
    pub items: Vec<TaskDto>,
}

/// 任务 run DTO。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskRunDto {
    pub id: i64,
    pub task_id: i64,
    pub owner_user_id: i64,
    pub trigger_type: String,
    pub idempotency_key: String,
    pub sea_tunnel_job_id: Option<String>,
    pub status: String,
    pub attempt: i32,
    pub start_time: Option<i64>,
    pub end_time: Option<i64>,
    pub error_message: Option<String>,
    pub log_path: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// 手动触发请求（可传覆盖参数；T1 仅幂等键语义）。
#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TriggerRequest {
    /// 客户端幂等键：同一请求重复提交命中同一 run（T1 用例 7）。
    pub idempotency_key: Option<String>,
    /// 覆盖参数（T1 预留，未实现）。
    pub overrides: Option<serde_json::Value>,
}
