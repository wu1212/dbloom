//! users 路由（02-api.md §2.2，全部 admin）。

use axum::{
    Extension, Json,
    extract::{Path, Query, State},
};
use dbloom_common::AppError;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::{
    auth::AuthCtx,
    error::{ok_json, ApiError},
    state::AppState,
};

fn require_admin(ctx: &AuthCtx) -> Result<(), ApiError> {
    if ctx.is_admin() {
        Ok(())
    } else {
        Err(ApiError::from(AppError::forbidden("仅管理员可执行此操作")))
    }
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct CreateUserBody {
    pub username: String,
    pub display_name: Option<String>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct CreateUserResponse {
    pub id: i64,
    pub username: String,
    /// 随机初始密码（仅此一次返回）。
    pub initial_password: String,
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct UpdateUserBody {
    pub display_name: Option<String>,
    pub status: Option<String>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct ResetPasswordResponse {
    pub new_password: String,
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct UserListQuery {
    pub keyword: Option<String>,
    pub status: Option<String>,
    pub page: Option<i64>,
    pub page_size: Option<i64>,
}

/// GET /api/v1/users
pub async fn list(
    State(state): State<Arc<AppState>>,
    ctx: Extension<AuthCtx>,
    Query(q): Query<UserListQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_admin(&ctx)?;
    let resp = state
        .iam
        .list_users(q.keyword.as_deref(), q.status.as_deref(), q.page, q.page_size)
        .await
        .map_err(ApiError::from)?;
    Ok(ok_json(resp))
}

/// POST /api/v1/users（仅普通用户；D8）
pub async fn create(
    State(state): State<Arc<AppState>>,
    ctx: Extension<AuthCtx>,
    Json(body): Json<CreateUserBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_admin(&ctx)?;
    let (user, initial_password) = state
        .iam
        .create_user(ctx.user_id(), &body.username, body.display_name.as_deref())
        .await
        .map_err(ApiError::from)?;
    Ok(ok_json(CreateUserResponse {
        id: user.id,
        username: user.username,
        initial_password,
    }))
}

/// GET /api/v1/users/{id}
pub async fn detail(
    State(state): State<Arc<AppState>>,
    ctx: Extension<AuthCtx>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_admin(&ctx)?;
    let user = state.iam.get_user(id).await.map_err(ApiError::from)?;
    Ok(ok_json(user))
}

/// PUT /api/v1/users/{id}
pub async fn update(
    State(state): State<Arc<AppState>>,
    ctx: Extension<AuthCtx>,
    Path(id): Path<i64>,
    Json(body): Json<UpdateUserBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_admin(&ctx)?;
    let user = state
        .iam
        .update_user(ctx.user_id(), id, body.display_name.as_deref(), body.status.as_deref())
        .await
        .map_err(ApiError::from)?;
    Ok(ok_json(user))
}

/// DELETE /api/v1/users/{id}（软删）
pub async fn delete(
    State(state): State<Arc<AppState>>,
    ctx: Extension<AuthCtx>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_admin(&ctx)?;
    state
        .iam
        .delete_user(ctx.user_id(), id)
        .await
        .map_err(ApiError::from)?;
    Ok(crate::error::ok_no_data())
}

/// POST /api/v1/users/{id}/reset-password（强制重置，D20）
pub async fn reset_password(
    State(state): State<Arc<AppState>>,
    ctx: Extension<AuthCtx>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_admin(&ctx)?;
    let new_password = state
        .iam
        .reset_password(ctx.user_id(), id)
        .await
        .map_err(ApiError::from)?;
    Ok(ok_json(ResetPasswordResponse { new_password }))
}
