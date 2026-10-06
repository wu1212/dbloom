//! 认证：全局中间件从 `Authorization: Bearer` 解析行为主体，注入 `AuthCtx`。
//!
//! D9：JWT（人机）与 API Key（机器）共用同一套路由，鉴权来源不同。
//! 配合 `dbloom-storage::TenantScope` 在数据访问层强制租户过滤（D8）。

use axum::{
    extract::{Request, State},
    http::header,
    middleware::Next,
    response::Response,
};
use dbloom_common::AppError;
use dbloom_iam::AuthResult;
use std::sync::Arc;

use crate::{error::ApiError, state::AppState};

/// 已认证的行为主体。
#[derive(Debug, Clone)]
pub struct AuthCtx(pub AuthResult);

impl AuthCtx {
    pub fn is_admin(&self) -> bool {
        matches!(self.0.role, dbloom_types::UserRole::Admin)
    }

    pub fn user_id(&self) -> i64 {
        self.0.user_id
    }
}

/// 认证中间件（挂载到受保护路由组）。失败直接返回 401/403。
pub async fn auth_middleware(
    State(app): State<Arc<AppState>>,
    mut req: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let authz = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            ApiError::from(AppError::unauthorized("缺少 Authorization 头（Bearer JWT 或 API Key）"))
        })?;

    let actor = app.iam.authenticate(&authz).await.map_err(ApiError::from)?;
    req.extensions_mut().insert(AuthCtx(actor));
    Ok(next.run(req).await)
}
