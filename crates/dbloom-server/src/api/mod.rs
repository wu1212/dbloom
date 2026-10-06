//! 路由装配（02-api.md §2.1–2.3 + health）。
//!
//! - 公开路由：`/health`、`/auth/login`、`/auth/refresh`、OpenAPI 文档。
//! - 受保护路由：其余全部（中间件认证后注入 `AuthCtx`）。

mod apikeys;
mod auth;
mod health;
mod users;

use axum::{
    middleware,
    routing::{get, post},
    Router,
};
use std::sync::Arc;

use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use crate::{openapi::ApiDoc, state::AppState};

/// 构建应用 Router。
pub fn build_router(state: Arc<AppState>) -> Router {
    // ---- 需要认证的路由（统一中间件） ----
    let protected = Router::new()
        .route("/auth/logout", post(auth::logout))
        .route("/auth/me", get(auth::me))
        .route("/auth/change-password", post(auth::change_password))
        .route("/users", get(users::list).post(users::create))
        .route(
            "/users/{id}",
            get(users::detail).put(users::update).delete(users::delete),
        )
        .route("/users/{id}/reset-password", post(users::reset_password))
        .route(
            "/users/{id}/api-keys",
            get(apikeys::list_for_user).post(apikeys::create),
        )
        .route(
            "/api-keys/{key_id}",
            axum::routing::put(apikeys::update).delete(apikeys::revoke),
        )
        .layer(middleware::from_fn_with_state(
            state.clone(),
            crate::auth::auth_middleware,
        ));

    // ---- 公开路由 ----
    let public = Router::new()
        .route("/health", get(health::health))
        .route("/auth/login", post(auth::login))
        .route("/auth/refresh", post(auth::refresh));

    let api = public.merge(protected);

    Router::new()
        .merge(
            SwaggerUi::new("/api/v1/docs").url("/api/v1/openapi.json", ApiDoc::openapi()),
        )
        .nest("/api/v1", api)
        .fallback(not_found)
        .with_state(state)
}

async fn not_found() -> (axum::http::StatusCode, axum::Json<serde_json::Value>) {
    (
        axum::http::StatusCode::NOT_FOUND,
        axum::Json(serde_json::json!({
            "code": 40400,
            "message": "资源不存在",
            "trace_id": "",
        })),
    )
}
