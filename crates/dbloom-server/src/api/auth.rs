//! auth 路由（02-api.md §2.1）。

use axum::{Extension, Json, extract::State};
use dbloom_common::AppError;
use dbloom_types::auth::{ChangePasswordRequest, LoginRequest, RefreshRequest};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::{
    auth::AuthCtx,
    error::{ok_json, ApiError},
    state::AppState,
};

#[derive(Debug, Deserialize, Serialize)]
pub struct LogoutRequest {
    pub refresh_token: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ChangePasswordResponse {
    pub success: bool,
    pub message: String,
}

/// POST /api/v1/auth/login（匿名）
#[axum::debug_handler]
pub async fn login(
    State(state): State<Arc<AppState>>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if req.username.trim().is_empty() || req.password.is_empty() {
        return Err(ApiError::from(AppError::validation("用户名与密码不能为空")));
    }
    let resp = state
        .iam
        .login(&req.username, &req.password, None)
        .await
        .map_err(ApiError::from)?;
    Ok(ok_json(resp))
}

/// POST /api/v1/auth/refresh（持有 refresh token）
pub async fn refresh(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RefreshRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let resp = state
        .iam
        .refresh(&req.refresh_token)
        .await
        .map_err(ApiError::from)?;
    Ok(ok_json(resp))
}

/// POST /api/v1/auth/logout（登录态）
pub async fn logout(
    State(state): State<Arc<AppState>>,
    Extension(actor): Extension<AuthCtx>,
    Json(req): Json<LogoutRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state
        .iam
        .logout(actor.user_id(), &req.refresh_token)
        .await
        .map_err(ApiError::from)?;
    Ok(ok_json(serde_json::json!({"success": true})))
}

/// GET /api/v1/auth/me（登录态）
pub async fn me(
    State(state): State<Arc<AppState>>,
    Extension(actor): Extension<AuthCtx>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let user = state.iam.get_user(actor.user_id()).await.map_err(ApiError::from)?;
    Ok(ok_json(user))
}

/// POST /api/v1/auth/change-password（登录态；含首登强改 D20）
pub async fn change_password(
    State(state): State<Arc<AppState>>,
    Extension(actor): Extension<AuthCtx>,
    Json(req): Json<ChangePasswordRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state
        .iam
        .change_password(actor.user_id(), &req.old_password, &req.new_password)
        .await
        .map_err(ApiError::from)?;
    Ok(ok_json(ChangePasswordResponse {
        success: true,
        message: "密码已更新，请重新登录".to_string(),
    }))
}
