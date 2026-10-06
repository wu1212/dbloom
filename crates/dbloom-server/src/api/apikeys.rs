//! API Key 路由（02-api.md §2.3，admin 域 D9）。

use axum::{
    Extension, Json,
    extract::{Path, State},
};
use dbloom_common::AppError;
use std::sync::Arc;

use crate::{
    auth::AuthCtx,
    error::{ok_json, ApiError},
    state::AppState,
};
use dbloom_types::apikey::{CreateApiKeyRequest, UpdateApiKeyRequest};

fn require_admin(ctx: &AuthCtx) -> Result<(), ApiError> {
    if ctx.is_admin() {
        Ok(())
    } else {
        Err(ApiError::from(AppError::forbidden("仅管理员可执行此操作")))
    }
}

/// GET /api/v1/users/{id}/api-keys：某用户的 Key 列表
pub async fn list_for_user(
    State(state): State<Arc<AppState>>,
    ctx: Extension<AuthCtx>,
    Path(user_id): Path<i64>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_admin(&ctx)?;
    let keys = state
        .iam
        .list_api_keys(user_id)
        .await
        .map_err(ApiError::from)?;
    Ok(ok_json(keys))
}

/// POST /api/v1/users/{id}/api-keys：签发 Key（明文仅返回一次）
pub async fn create(
    State(state): State<Arc<AppState>>,
    ctx: Extension<AuthCtx>,
    Path(user_id): Path<i64>,
    Json(req): Json<CreateApiKeyRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_admin(&ctx)?;
    let resp = state
        .iam
        .issue_api_key(ctx.user_id(), user_id, req, None)
        .await
        .map_err(ApiError::from)?;
    Ok(ok_json(resp))
}

/// PUT /api/v1/api-keys/{key_id}：更新 name/status/生效/失效时间
pub async fn update(
    State(state): State<Arc<AppState>>,
    ctx: Extension<AuthCtx>,
    Path(key_id): Path<i64>,
    Json(req): Json<UpdateApiKeyRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_admin(&ctx)?;
    let key = state
        .iam
        .update_api_key(ctx.user_id(), key_id, req)
        .await
        .map_err(ApiError::from)?;
    Ok(ok_json(key))
}

/// DELETE /api/v1/api-keys/{key_id}：撤销 Key（立即全局生效）
pub async fn revoke(
    State(state): State<Arc<AppState>>,
    ctx: Extension<AuthCtx>,
    Path(key_id): Path<i64>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_admin(&ctx)?;
    state
        .iam
        .revoke_api_key(ctx.user_id(), key_id)
        .await
        .map_err(ApiError::from)?;
    Ok(crate::error::ok_no_data())
}
