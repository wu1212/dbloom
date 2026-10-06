//! Tasks / Runs 路由（`docs/design/02-api.md` §2.6，D4/D5；M3 最小垂直切片）。
//!
//! - 鉴权：走统一中间件；**仅有 owner 或 admin 可见其任务**，越权一律 404（防存在性泄漏）。
//! - HOCON：创建任务时渲染并存脱敏快照（`config_hocon`）；触发时用**全量 HOCON（含密码）**提交引擎。
//! - 幂等：`task_runs.idempotency_key` 唯一，重复提交命中同一 run（T1 用例 7）。
//! - 引擎：`submit_job` → 轮询 `job-status` → 回写 run（pending→running→succeeded/failed/canceled）。

use axum::{
    Extension, Json,
    extract::{Path, Query, State},
};
use dbloom_common::{AppError, time::now_ms};
use dbloom_storage::dao::{ConnectionRow, TaskRunRow};
use dbloom_storage::tenant::TenantScope;
use dbloom_sync::{EngineClient, JdbcConn, TableMapping};
use dbloom_types::{
    CreateTaskRequest, TaskDto, TaskRunDto, TriggerRequest, UpdateTaskRequest,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::{
    auth::AuthCtx,
    error::{ok_json, ApiError, ok_no_data},
    state::AppState,
};

// ---------------- 辅助 ----------------

fn task_row_to_dto(row: &dbloom_storage::dao::TaskRow) -> TaskDto {
    TaskDto {
        id: row.id,
        owner_user_id: row.owner_user_id,
        name: row.name.clone(),
        description: row.description.clone(),
        source_connection_id: row.source_connection_id,
        sink_connection_id: row.sink_connection_id,
        sync_mode: row.sync_mode.clone(),
        config_hocon: row.config_hocon.clone(),
        schedule_cron: row.schedule_cron.clone(),
        enabled: row.enabled,
        timeout_sec: row.timeout_sec,
        retry_times: row.retry_times,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

fn run_row_to_dto(row: &TaskRunRow) -> TaskRunDto {
    TaskRunDto {
        id: row.id,
        task_id: row.task_id,
        owner_user_id: row.owner_user_id,
        trigger_type: row.trigger_type.clone(),
        idempotency_key: row.idempotency_key.clone(),
        sea_tunnel_job_id: row.sea_tunnel_job_id.clone(),
        status: row.status.clone(),
        attempt: row.attempt,
        start_time: row.start_time,
        end_time: row.end_time,
        error_message: row.error_message.clone(),
        log_path: row.log_path.clone(),
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

/// 加载一条「当前可见」任务：非 owner/管理员见不到 → 404（防存在性泄漏）。
pub(crate) async fn load_visible_task(
    state: &AppState,
    ctx: &AuthCtx,
    id: i64,
) -> Result<dbloom_storage::dao::TaskRow, ApiError> {
    let row = state.tasks.get_by_id(id).await.map_err(ApiError::from)?;
    let scope = TenantScope::actor(ctx.user_id(), ctx.is_admin());
    if !scope.can_access_owner(row.owner_user_id) {
        return Err(ApiError::from(AppError::not_found("任务不存在")));
    }
    Ok(row)
}

/// 加载可见连接（复用 connections::load_visible，返回 ConnectionRow）。
async fn load_visible_conn(
    state: &AppState,
    ctx: &AuthCtx,
    id: i64,
) -> Result<ConnectionRow, ApiError> {
    crate::api::connections::load_visible(state, ctx, id).await
}

/// 从连接行构造 JdbcConn（解密密码，仅内存）。
fn jdbc_conn_from_row(state: &AppState, row: &ConnectionRow) -> Result<JdbcConn, ApiError> {
    let password = match row.password_enc.as_deref() {
        Some(enc) => Some(state.crypto.decrypt_str(enc).map_err(ApiError::from)?),
        None => None,
    };
    Ok(JdbcConn {
        conn_type: row.conn_type.clone(),
        host: row.host.clone(),
        port: row.port,
        database: row.database_name.clone(),
        username: row.username.clone(),
        password,
    })
}

/// T1 仅支持 batch（全量）同步。
fn validate_sync_mode(mode: &str) -> Result<(), ApiError> {
    if mode != "batch" {
        return Err(ApiError::from(AppError::validation(format!(
            "同步类型 {mode} 当前版本不支持（T1 仅 batch 全量；increment/cdc 于 M4 透传引擎能力）"
        ))));
    }
    Ok(())
}

/// 由请求体构造 TableMapping 列表。
fn table_mappings_of(req: &CreateTaskRequest) -> Result<Vec<TableMapping>, ApiError> {
    let mut out = Vec::with_capacity(req.table_mapping.len());
    for m in &req.table_mapping {
        if m.source_table.trim().is_empty() || m.sink_table.trim().is_empty() {
            return Err(ApiError::from(AppError::validation("表映射不能含空表名")));
        }
        out.push(TableMapping {
            source_table: m.source_table.trim().to_string(),
            sink_table: m.sink_table.trim().to_string(),
        });
    }
    Ok(out)
}

/// 把 TableMapping 序列化为 JSON 文本（存 tasks.table_mapping）。
fn mappings_to_json(mappings: &[TableMapping]) -> String {
    let arr: Vec<serde_json::Value> = mappings
        .iter()
        .map(|m| {
            serde_json::json!({ "sourceTable": m.source_table, "sinkTable": m.sink_table })
        })
        .collect();
    serde_json::to_string(&arr).unwrap_or_else(|_| "[]".into())
}

/// 从 TaskRow 解析表映射（JSON 列）。
fn parse_task_mappings(row: &dbloom_storage::dao::TaskRow) -> Result<Vec<TableMapping>, ApiError> {
    let text = row.table_mapping.as_deref().unwrap_or("[]");
    let v: serde_json::Value = serde_json::from_str(text)
        .map_err(|e| ApiError::from(AppError::internal(format!("任务表映射解析失败: {e}"))))?;
    dbloom_sync::parse_mappings(&v)
        .map_err(|e| ApiError::from(AppError::internal(format!("任务表映射无效: {e}"))))
}

/// 渲染任务 HOCON 的**脱敏快照**（create 时落库，供回看；触发时再渲染含密码全量）。
async fn render_task_snapshot(
    state: &AppState,
    ctx: &AuthCtx,
    src_id: i64,
    sink_id: i64,
    mappings: &[TableMapping],
) -> Result<String, ApiError> {
    let src_row = load_visible_conn(state, ctx, src_id).await?;
    let sink_row = load_visible_conn(state, ctx, sink_id).await?;
    let src = jdbc_conn_from_row(state, &src_row)?;
    let sink = jdbc_conn_from_row(state, &sink_row)?;
    let rendered = dbloom_sync::render_full_jdbc(&src, &sink, mappings, &Default::default())
        .map_err(|e| ApiError::from(AppError::validation(e)))?;
    Ok(rendered.snapshot_hocon)
}

/// spawn：异步把 run 跑到底（提交引擎 + 轮询回写）。
fn spawn_run(state: &AppState, task_row: &dbloom_storage::dao::TaskRow, run_id: i64, full_hocon: String) {
    let runs = Arc::new(state.runs.clone());
    let engine: Arc<EngineClient> = state.engine.clone();
    let task_id = task_row.id;
    let name = task_row.name.clone();
    let timeout_sec = task_row.timeout_sec;
    tokio::spawn(async move {
        dbloom_sync::submit_and_wait(
            runs,
            engine,
            run_id,
            task_id,
            &name,
            &full_hocon,
            timeout_sec,
        )
        .await;
    });
}

// ---------------- 请求 DTO ----------------

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskListQuery {
    pub keyword: Option<String>,
    pub sync_mode: Option<String>,
    pub enabled: Option<bool>,
    pub user_id: Option<i64>,
    pub page: Option<i64>,
    pub page_size: Option<i64>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunListQuery {
    pub user_id: Option<i64>,
    pub page: Option<i64>,
    pub page_size: Option<i64>,
}

// ---------------- Handlers ----------------

/// GET /api/v1/tasks
pub async fn list(
    State(state): State<Arc<AppState>>,
    ctx: Extension<AuthCtx>,
    Query(q): Query<TaskListQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let scope = tenant_scope(&ctx, q.user_id)?;
    let (total, rows) = state
        .tasks
        .list(&scope, q.keyword.as_deref(), q.sync_mode.as_deref(), q.enabled, q.page, q.page_size)
        .await
        .map_err(ApiError::from)?;
    let items: Vec<TaskDto> = rows.iter().map(task_row_to_dto).collect();
    Ok(ok_json(serde_json::json!({ "total": total, "items": items })))
}

/// POST /api/v1/tasks —— 创建任务（T1：HOCON 渲染 + 快照留存）。
pub async fn create(
    State(state): State<Arc<AppState>>,
    ctx: Extension<AuthCtx>,
    Json(body): Json<CreateTaskRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if body.name.trim().is_empty() {
        return Err(ApiError::from(AppError::validation("任务名称不能为空")));
    }
    validate_sync_mode(&body.sync_mode)?;
    let mappings = table_mappings_of(&body)?;
    if mappings.is_empty() {
        return Err(ApiError::from(AppError::validation("至少需要一条表映射")));
    }

    // 校验源/目标可见 + 渲染脱敏快照（供回看；全量 HOCON 触发时再重建）
    let snapshot = render_task_snapshot(
        &state,
        &ctx,
        body.source_connection_id,
        body.sink_connection_id,
        &mappings,
    )
    .await?;

    let row = state
        .tasks
        .create(
            ctx.user_id(),
            body.name.trim(),
            body.description.as_deref().map(str::trim),
            body.source_connection_id,
            body.sink_connection_id,
            &body.sync_mode,
            &snapshot,
            Some(&mappings_to_json(&mappings)),
            body.schedule_cron.as_deref().map(str::trim),
            body.enabled,
            body.timeout_sec.max(0),
            body.retry_times.max(0),
            1,
            Some(ctx.user_id()),
            now_ms(),
        )
        .await
        .map_err(ApiError::from)?;

    state
        .iam
        .record_audit(
            Some(ctx.user_id()),
            "user",
            "task_create",
            Some("task"),
            Some(&row.id.to_string()),
            Some(serde_json::json!({ "name": row.name, "sync_mode": row.sync_mode, "source_connection_id": row.source_connection_id, "sink_connection_id": row.sink_connection_id })),
            None,
        )
        .await
        .map_err(ApiError::from)?;

    Ok(ok_json(task_row_to_dto(&row)))
}

/// GET /api/v1/tasks/{id} —— 任务详情（含 HOCON 快照）。
pub async fn detail(
    State(state): State<Arc<AppState>>,
    ctx: Extension<AuthCtx>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let row = load_visible_task(&state, &ctx, id).await?;
    Ok(ok_json(task_row_to_dto(&row)))
}

/// PUT /api/v1/tasks/{id} —— 更新任务（改参须重新渲染 HOCON 快照；T1 仅允许改描述/超时/重试次数/开关）。
pub async fn update(
    State(state): State<Arc<AppState>>,
    ctx: Extension<AuthCtx>,
    Path(id): Path<i64>,
    Json(body): Json<UpdateTaskRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let _row = load_visible_task(&state, &ctx, id).await?;
    if let Some(m) = &body.sync_mode {
        validate_sync_mode(m)?;
    }
    if let Some(n) = &body.name {
        if n.trim().is_empty() {
            return Err(ApiError::from(AppError::validation("任务名称不能为空")));
        }
    }

    let upd = dbloom_storage::dao::TaskUpdate {
        name: body.name.as_ref().map(|s| s.trim().to_string()),
        description: Some(body.description.clone()),
        sync_mode: body.sync_mode.clone(),
        config_hocon: None, // 源/目标不变，快照不变
        schedule_cron: Some(body.schedule_cron.clone()),
        enabled: body.enabled,
        timeout_sec: body.timeout_sec,
        retry_times: body.retry_times,
    };
    // 若 name/sync 未变则保留原值由 DAO 合并
    let updated = state.tasks.update(id, &upd, now_ms()).await.map_err(ApiError::from)?;

    state
        .iam
        .record_audit(
            Some(ctx.user_id()),
            "user",
            "task_update",
            Some("task"),
            Some(&id.to_string()),
            Some(serde_json::json!({ "name": updated.name })),
            None,
        )
        .await
        .map_err(ApiError::from)?;

    Ok(ok_json(task_row_to_dto(&updated)))
}

/// DELETE /api/v1/tasks/{id} —— 软删（有活跃 run 时先拒绝）。
pub async fn delete(
    State(state): State<Arc<AppState>>,
    ctx: Extension<AuthCtx>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let row = load_visible_task(&state, &ctx, id).await?;
    let scope = TenantScope::actor(ctx.user_id(), ctx.is_admin());
    if let Some(active) = state.runs.active_run(&scope, id).await.map_err(ApiError::from)? {
        let _ = active;
        return Err(ApiError::from(AppError::conflict("任务有运行中实例，请先停止再删除")));
    }
    state.tasks.soft_delete(id, now_ms()).await.map_err(ApiError::from)?;
    state
        .iam
        .record_audit(
            Some(ctx.user_id()),
            "user",
            "task_delete",
            Some("task"),
            Some(&id.to_string()),
            Some(serde_json::json!({ "name": row.name })),
            None,
        )
        .await
        .map_err(ApiError::from)?;
    Ok(ok_no_data())
}

/// POST /api/v1/tasks/{id}/enable | disable —— 调度开关。
pub async fn set_enabled(
    State(state): State<Arc<AppState>>,
    ctx: Extension<AuthCtx>,
    Path(id): Path<i64>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let enabled = body.get("enabled").and_then(serde_json::Value::as_bool).unwrap_or(true);
    let _row = load_visible_task(&state, &ctx, id).await?;
    state.tasks.set_enabled(id, enabled, now_ms()).await.map_err(ApiError::from)?;
    Ok(ok_json(serde_json::json!({ "id": id, "enabled": enabled })))
}

/// POST /api/v1/tasks/{id}/trigger —— 手动触发（幂等 + 异步跑）。
pub async fn trigger(
    State(state): State<Arc<AppState>>,
    ctx: Extension<AuthCtx>,
    Path(id): Path<i64>,
    Json(body): Json<TriggerRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let task = load_visible_task(&state, &ctx, id).await?;
    // 并发上限校验（concurrency_limit=1：已有 running run 则拒绝重复触发）
    let scope = TenantScope::actor(ctx.user_id(), ctx.is_admin());
    if let Some(active) = state.runs.active_run(&scope, id).await.map_err(ApiError::from)? {
        return Err(ApiError::from(AppError::conflict(format!(
            "任务已有运行中实例（run {}），请等待完成或先停止",
            active.id
        ))));
    }

    // 幂等键：客户端传入或服务端生成
    let idem = body
        .idempotency_key
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| format!("trigger-{}-{}", id, now_ms()));

    let run_id = trigger_inner(&state, &ctx, &task, &idem).await?;
    Ok(ok_json(serde_json::json!({ "runId": run_id })))
}

/// trigger 的具体执行（被 retry 复用）。返回新 run Id。
async fn trigger_inner(
    state: &AppState,
    ctx: &AuthCtx,
    task: &dbloom_storage::dao::TaskRow,
    idem: &str,
) -> Result<i64, ApiError> {
    // 渲染全量 HOCON（含密码，仅进程内存）
    let mappings = parse_task_mappings(task)?;
    let src_row = load_visible_conn(state, ctx, task.source_connection_id).await?;
    let sink_row = load_visible_conn(state, ctx, task.sink_connection_id).await?;
    let src = jdbc_conn_from_row(state, &src_row)?;
    let sink = jdbc_conn_from_row(state, &sink_row)?;
    let rendered = dbloom_sync::render_full_jdbc(&src, &sink, &mappings, &Default::default())
        .map_err(|e| ApiError::from(AppError::validation(e)))?;

    let attempt = state
        .runs
        .latest_for_task(task.id)
        .await
        .map_err(ApiError::from)?
        .map(|r| r.attempt + 1)
        .unwrap_or(1);

    let log_path = Some(format!("logs/tasks/{}.log", uuid::Uuid::new_v4()));
    let run = state
        .runs
        .create_pending(task.id, task.owner_user_id, "manual", idem, attempt, log_path.as_deref(), now_ms())
        .await
        .map_err(ApiError::from)?;

    // 幂等命中：直接返回既有 run 的 id（同请求重复提交不新建）
    let run_id = match run {
        Some(r) => r.id,
        None => {
            let exist = state
                .runs
                .latest_for_task(task.id)
                .await
                .map_err(ApiError::from)?
                .ok_or_else(|| ApiError::from(AppError::internal("幂等命中但找不到既有 run")))?;
            return Ok(exist.id);
        }
    };

    state
        .iam
        .record_audit(
            Some(ctx.user_id()),
            "user",
            "task_trigger",
            Some("task"),
            Some(&task.id.to_string()),
            Some(serde_json::json!({ "run_id": run_id, "idempotency": idem })),
            None,
        )
        .await
        .map_err(ApiError::from)?;

    spawn_run(state, task, run_id, rendered.full_hocon);
    Ok(run_id)
}

/// POST /api/v1/tasks/{id}/stop —— 停止当前运行中的 run（真停）。
pub async fn stop(
    State(state): State<Arc<AppState>>,
    ctx: Extension<AuthCtx>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let _task = load_visible_task(&state, &ctx, id).await?;
    let scope = TenantScope::actor(ctx.user_id(), ctx.is_admin());
    let active = state
        .runs
        .active_run(&scope, id)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::from(AppError::conflict("任务当前没有运行中的实例")))?;

    let runs = Arc::new(state.runs.clone());
    let engine = state.engine.clone();
    dbloom_sync::stop_run(runs, engine, &active)
        .await
        .map_err(|e| ApiError::from(AppError::internal(format!("停止任务失败: {e}"))))?;

    state
        .iam
        .record_audit(
            Some(ctx.user_id()),
            "user",
            "task_stop",
            Some("task_run"),
            Some(&active.id.to_string()),
            Some(serde_json::json!({ "task_id": id })),
            None,
        )
        .await
        .map_err(ApiError::from)?;

    Ok(ok_json(serde_json::json!({ "runId": active.id, "status": "canceled" })))
}

/// POST /api/v1/tasks/{id}/retry —— 重试最近一次失败 run（新建 run，attempt+1）。
pub async fn retry(
    State(state): State<Arc<AppState>>,
    ctx: Extension<AuthCtx>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let task = load_visible_task(&state, &ctx, id).await?;
    let scope = TenantScope::actor(ctx.user_id(), ctx.is_admin());
    let latest = state
        .runs
        .latest_for_task(id)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::from(AppError::conflict("任务尚无运行历史，无法重试")))?;
    if latest.status != "failed" && latest.status != "stopped" {
        return Err(ApiError::from(AppError::conflict(format!(
            "仅最近一次失败/停止的 run 可重试（当前状态: {}）",
            latest.status
        ))));
    }

    let _ = &scope;
    let idem = dbloom_sync::retry_idempotency_key();
    let run_id = trigger_inner(&state, &ctx, &task, &idem).await?;

    state
        .iam
        .record_audit(
            Some(ctx.user_id()),
            "user",
            "task_retry",
            Some("task"),
            Some(&id.to_string()),
            Some(serde_json::json!({ "from_run": latest.id, "new_run": run_id })),
            None,
        )
        .await
        .map_err(ApiError::from)?;

    Ok(ok_json(serde_json::json!({ "runId": run_id })))
}

/// GET /api/v1/tasks/{id}/runs —— 运行历史。
pub async fn runs(
    State(state): State<Arc<AppState>>,
    ctx: Extension<AuthCtx>,
    Path(id): Path<i64>,
    Query(q): Query<RunListQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let _task = load_visible_task(&state, &ctx, id).await?;
    let scope = tenant_scope(&ctx, q.user_id)?;
    let (total, rows) = state
        .runs
        .list_by_task(&scope, id, q.page, q.page_size)
        .await
        .map_err(ApiError::from)?;
    let items: Vec<TaskRunDto> = rows.iter().map(run_row_to_dto).collect();
    Ok(ok_json(serde_json::json!({ "total": total, "items": items })))
}

/// GET /api/v1/runs/{runId} —— 单次 run 详情。
pub async fn run_detail(
    State(state): State<Arc<AppState>>,
    ctx: Extension<AuthCtx>,
    Path(run_id): Path<i64>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let run = state.runs.get_by_id(run_id).await.map_err(ApiError::from)?;
    let scope = TenantScope::actor(ctx.user_id(), ctx.is_admin());
    if !scope.can_access_owner(run.owner_user_id) {
        return Err(ApiError::from(AppError::not_found("运行记录不存在")));
    }
    Ok(ok_json(run_row_to_dto(&run)))
}

/// GET /api/v1/runs/{runId}/logs —— run 日志（T1：返回本地 log 文件 + 错误信息；不直连引擎日志流）。
pub async fn run_logs(
    State(state): State<Arc<AppState>>,
    ctx: Extension<AuthCtx>,
    Path(run_id): Path<i64>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let run = state.runs.get_by_id(run_id).await.map_err(ApiError::from)?;
    let scope = TenantScope::actor(ctx.user_id(), ctx.is_admin());
    if !scope.can_access_owner(run.owner_user_id) {
        return Err(ApiError::from(AppError::not_found("运行记录不存在")));
    }

    // 从共享卷 logs/ 读取（log_path 形如 logs/tasks/<uuid>.log）
    let mut content = String::new();
    if let Some(lp) = &run.log_path {
        let root = std::env::var("DBLOOM_DATA_ROOT").unwrap_or_else(|_| "shared".into());
        let path = std::path::Path::new(&root).join(lp);
        if path.exists() {
            content = std::fs::read_to_string(&path).unwrap_or_else(|_| "(读取日志失败)".into());
        }
    }
    if content.is_empty() {
        content = run
            .error_message
            .clone()
            .unwrap_or_else(|| "(暂无日志：运行尚未产生或已清理)".into());
    }

    Ok(ok_json(serde_json::json!({
        "runId": run.id,
        "status": run.status,
        "log": content,
    })))
}

/// GET /api/v1/tasks/{id}/dag —— T1 返回空 DAG（M5 前任务依赖编排）。
pub async fn dag(
    State(state): State<Arc<AppState>>,
    ctx: Extension<AuthCtx>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let _row = load_visible_task(&state, &ctx, id).await?;
    Ok(ok_json(serde_json::json!({ "taskId": id, "dependencies": [], "downstreams": [] })))
}

// ---- tenant_scope 辅助（本文件内本地副本） ----

fn tenant_scope(ctx: &AuthCtx, explicit_user_id: Option<i64>) -> Result<TenantScope, ApiError> {
    if let Some(uid) = explicit_user_id {
        if !ctx.is_admin() {
            return Err(ApiError::from(AppError::forbidden(
                "普通用户不可跨用户查询（user_id 仅管理员可用）",
            )));
        }
        return Ok(TenantScope::admin_explicit(ctx.user_id(), uid));
    }
    Ok(TenantScope::actor(ctx.user_id(), ctx.is_admin()))
}
