//! connections 路由（`docs/design/02-api.md` §2.4，D6/D7/D17）。
//!
//! 安全约定（04-security §2.2/§3.1）：
//! - 单条资源越权一律 **404**（不泄漏存在性：`load_visible`，非 owner 返回「连接不存在」）；
//! - **任何响应不回传密码**；明文密码只出现在请求体与加密前的进程内存；
//! - 列表按 `TenantScope` 注入 `owner_user_id`（DAO 层强制过滤，admin 可 `?user_id=`）。

use axum::{
    Extension, Json,
    extract::{Path, Query, State},
};
use dbloom_common::{AppError, time::now_ms};
use dbloom_storage::dao::{ConnectionRow, ConnectionUpdate};
use dbloom_storage::tenant::TenantScope;
use dbloom_types::{
    ConnectionDto,
    manifest::{all_manifests, is_supported_type},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::{
    auth::AuthCtx,
    error::{ok_json, ApiError, ok_no_data},
    state::AppState,
};

// ---------------- 辅助 ----------------

/// 由请求方构建租户作用域；普通用户传 `user_id` 一律 403（admin 可显式跨用户，D8）。
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

/// 加载一条「当前可见」的连接：非 owner/管理员见不到 → 404（防存在性泄漏）。
async fn load_visible(
    state: &AppState,
    ctx: &AuthCtx,
    id: i64,
) -> Result<ConnectionRow, ApiError> {
    let row = state
        .connections
        .get_by_id(id)
        .await
        .map_err(ApiError::from)?;
    let scope = TenantScope::actor(ctx.user_id(), ctx.is_admin());
    if !scope.can_access_owner(row.owner_user_id) {
        return Err(ApiError::from(AppError::not_found("连接不存在")));
    }
    Ok(row)
}

/// 行 → 脱敏 DTO（不含 password_enc；extra_params JSON 文本解析为对象）。
fn row_to_dto(row: &ConnectionRow) -> ConnectionDto {
    ConnectionDto {
        id: row.id,
        owner_user_id: row.owner_user_id,
        name: row.name.clone(),
        conn_type: row.conn_type.clone(),
        host: row.host.clone(),
        port: row.port,
        database_name: row.database_name.clone(),
        username: row.username.clone(),
        ssl_mode: row.ssl_mode.clone(),
        extra_params: row.extra_params.as_deref().and_then(|s| serde_json::from_str(s).ok()),
        is_production: row.is_production,
        read_only_lock: row.read_only_lock,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

fn validate_create(name: &str, conn_type: &str, host: &str) -> Result<(), ApiError> {
    if name.trim().is_empty() {
        return Err(ApiError::from(AppError::validation("连接名称不能为空")));
    }
    if host.trim().is_empty() {
        return Err(ApiError::from(AppError::validation("主机不能为空")));
    }
    if !is_supported_type(conn_type) {
        return Err(ApiError::from(AppError::validation(format!(
            "不支持的连接类型: {conn_type}（支持: mysql/postgres/sqlserver/mongodb/redis/elasticsearch）"
        ))));
    }
    Ok(())
}

// ---------------- 请求 DTO ----------------

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionListQuery {
    pub keyword: Option<String>,
    pub conn_type: Option<String>,
    pub user_id: Option<i64>,
    pub page: Option<i64>,
    pub page_size: Option<i64>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LockBody {
    /// 危险操作二次确认参数（D6）：lock/unlock 需 confirm=true。
    pub confirm: Option<bool>,
}

// ---------------- Handlers ----------------

/// GET /api/v1/connections
pub async fn list(
    State(state): State<Arc<AppState>>,
    ctx: Extension<AuthCtx>,
    Query(q): Query<ConnectionListQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let scope = tenant_scope(&ctx, q.user_id)?;
    let (total, rows) = state
        .connections
        .list(&scope, q.keyword.as_deref(), q.conn_type.as_deref(), q.page, q.page_size)
        .await
        .map_err(ApiError::from)?;
    let items: Vec<ConnectionDto> = rows.iter().map(row_to_dto).collect();
    Ok(ok_json(serde_json::json!({ "total": total, "items": items })))
}

/// POST /api/v1/connections
pub async fn create(
    State(state): State<Arc<AppState>>,
    ctx: Extension<AuthCtx>,
    Json(body): Json<dbloom_types::CreateConnectionRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    validate_create(&body.name, &body.conn_type, &body.host)?;

    let password_enc = match body.password.as_deref().filter(|p| !p.is_empty()) {
        Some(plain) => Some(
            state
                .crypto
                .encrypt_str(plain)
                .map_err(ApiError::from)?,
        ),
        None => None,
    };
    let extra_json = body
        .extra_params
        .as_ref()
        .map(|v| serde_json::to_string(v).unwrap_or_else(|_| "null".into()));

    let row = state
        .connections
        .create(
            ctx.user_id(),
            body.name.trim(),
            &body.conn_type,
            body.host.trim(),
            body.port,
            body.database_name.as_deref().map(str::trim),
            body.username.as_deref().map(str::trim),
            password_enc.as_deref(),
            body.ssl_mode.as_deref().unwrap_or("disable"),
            extra_json.as_deref(),
            body.is_production.unwrap_or(false),
            body.read_only_lock.unwrap_or(false),
            now_ms(),
        )
        .await
        .map_err(ApiError::from)?;

    state
        .iam
        .record_audit(
            Some(ctx.user_id()),
            "user",
            "conn_create",
            Some("connection"),
            Some(&row.id.to_string()),
            Some(serde_json::json!({ "name": row.name, "conn_type": row.conn_type, "host": row.host })),
            None,
        )
        .await
        .map_err(ApiError::from)?;

    Ok(ok_json(row_to_dto(&row)))
}

/// GET /api/v1/connections/types（manifest，D7 单一事实来源）
pub async fn types() -> Result<Json<serde_json::Value>, ApiError> {
    Ok(ok_json(all_manifests()))
}

/// GET /api/v1/connections/{id}
pub async fn detail(
    State(state): State<Arc<AppState>>,
    ctx: Extension<AuthCtx>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let row = load_visible(&state, &ctx, id).await?;
    Ok(ok_json(row_to_dto(&row)))
}

/// PUT /api/v1/connections/{id}（password 传非空 → 重加密；其余 None 保持原值）
pub async fn update(
    State(state): State<Arc<AppState>>,
    ctx: Extension<AuthCtx>,
    Path(id): Path<i64>,
    Json(body): Json<dbloom_types::UpdateConnectionRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let row = load_visible(&state, &ctx, id).await?;
    let _ = &row; // 归于可见性校验
    if let Some(name) = &body.name {
        if name.trim().is_empty() {
            return Err(ApiError::from(AppError::validation("连接名称不能为空")));
        }
    }
    if let Some(host) = &body.host {
        if host.trim().is_empty() {
            return Err(ApiError::from(AppError::validation("主机不能为空")));
        }
    }

    let password_enc = match body.password.as_deref().filter(|p| !p.is_empty()) {
        Some(plain) => Some(state.crypto.encrypt_str(plain).map_err(ApiError::from)?),
        None => None, // 不传密码 = 保持原密文（由 DAO 合并）
    };
    let extra_params = body
        .extra_params
        .as_ref()
        .map(|v| serde_json::to_string(v).unwrap_or_else(|_| "null".into()));

    let upd = ConnectionUpdate {
        name: body.name.as_ref().map(|s| s.trim().to_string()),
        host: body.host.as_ref().map(|s| s.trim().to_string()),
        port: body.port,
        database_name: body.database_name.as_ref().map(|s| s.trim().to_string()),
        username: body.username.as_ref().map(|s| s.trim().to_string()),
        password_enc,
        ssl_mode: body.ssl_mode.clone(),
        extra_params: Some(extra_params), // 始终以请求为准（含清空）
        is_production: body.is_production,
        read_only_lock: body.read_only_lock,
    };
    let updated = state
        .connections
        .update(id, &upd, now_ms())
        .await
        .map_err(ApiError::from)?;

    state
        .iam
        .record_audit(
            Some(ctx.user_id()),
            "user",
            "conn_update",
            Some("connection"),
            Some(&id.to_string()),
            Some(serde_json::json!({ "name": updated.name, "password_changed": body.password.is_some() })),
            None,
        )
        .await
        .map_err(ApiError::from)?;

    Ok(ok_json(row_to_dto(&updated)))
}

/// DELETE /api/v1/connections/{id}（软删，M1 无任务引用校验；M3 任务表落地后补）
pub async fn delete(
    State(state): State<Arc<AppState>>,
    ctx: Extension<AuthCtx>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let row = load_visible(&state, &ctx, id).await?;
    state
        .connections
        .soft_delete(id, now_ms())
        .await
        .map_err(ApiError::from)?;
    state
        .iam
        .record_audit(
            Some(ctx.user_id()),
            "user",
            "conn_delete",
            Some("connection"),
            Some(&id.to_string()),
            Some(serde_json::json!({ "name": row.name })),
            None,
        )
        .await
        .map_err(ApiError::from)?;
    Ok(ok_no_data())
}

/// POST /api/v1/connections/{id}/test（D17：server 端发起，前端永不直连）
pub async fn test(
    State(state): State<Arc<AppState>>,
    ctx: Extension<AuthCtx>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let row = load_visible(&state, &ctx, id).await?;

    // 解密凭据（进程内存，不落日志/不返前端）
    let password = match row.password_enc.as_deref() {
        Some(enc) => Some(state.crypto.decrypt_str(enc).map_err(ApiError::from)?),
        None => None,
    };

    let params = dbloom_connector::ConnParams {
        conn_type: row.conn_type.clone(),
        host: row.host.clone(),
        port: row.port.map(|p| p as u16),
        database: row.database_name.clone(),
        username: row.username.clone(),
        password,
        ssl_mode: row.ssl_mode.clone(),
    };
    let outcome = dbloom_connector::test_connection(&params)
        .await
        .map_err(ApiError::from)?;

    state
        .iam
        .record_audit(
            Some(ctx.user_id()),
            "user",
            "conn_test",
            Some("connection"),
            Some(&id.to_string()),
            Some(serde_json::json!({ "success": matches!(outcome, dbloom_connector::TestOutcome::Success { .. }) })),
            None,
        )
        .await
        .map_err(ApiError::from)?;

    Ok(ok_json(outcome.to_dto()))
}

/// POST /api/v1/connections/{id}/lock（生产只读锁，D6；危险操作需 confirm=true）
pub async fn lock(
    State(state): State<Arc<AppState>>,
    ctx: Extension<AuthCtx>,
    Path(id): Path<i64>,
    Json(body): Json<LockBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if body.confirm != Some(true) {
        return Err(ApiError::from(AppError::validation(
            "启用只读锁是危险操作，需 confirm=true 二次确认（D6）",
        )));
    }
    let row = load_visible(&state, &ctx, id).await?;
    state
        .connections
        .set_read_only_lock(id, true, now_ms())
        .await
        .map_err(ApiError::from)?;
    state
        .iam
        .record_audit(
            Some(ctx.user_id()),
            "user",
            "conn_lock",
            Some("connection"),
            Some(&id.to_string()),
            Some(serde_json::json!({ "lock": true, "name": row.name })),
            None,
        )
        .await
        .map_err(ApiError::from)?;
    Ok(ok_json(serde_json::json!({ "id": id, "readOnlyLock": true })))
}

/// POST /api/v1/connections/{id}/unlock（解除只读锁，D6）
pub async fn unlock(
    State(state): State<Arc<AppState>>,
    ctx: Extension<AuthCtx>,
    Path(id): Path<i64>,
    Json(body): Json<LockBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if body.confirm != Some(true) {
        return Err(ApiError::from(AppError::validation(
            "解除只读锁是危险操作，需 confirm=true 二次确认（D6）",
        )));
    }
    let row = load_visible(&state, &ctx, id).await?;
    state
        .connections
        .set_read_only_lock(id, false, now_ms())
        .await
        .map_err(ApiError::from)?;
    state
        .iam
        .record_audit(
            Some(ctx.user_id()),
            "user",
            "conn_lock",
            Some("connection"),
            Some(&id.to_string()),
            Some(serde_json::json!({ "lock": false, "name": row.name })),
            None,
        )
        .await
        .map_err(ApiError::from)?;
    Ok(ok_json(serde_json::json!({ "id": id, "readOnlyLock": false })))
}
