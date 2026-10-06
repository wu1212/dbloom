//! 路由装配（02-api.md §2.1–2.3 + health）。
//!
//! - 公开路由：`/health`、`/auth/login`、`/auth/refresh`、OpenAPI 文档。
//! - 受保护路由：其余全部（中间件认证后注入 `AuthCtx`）。

mod apikeys;
mod auth;
mod connections;
mod data;
mod export;
mod health;
mod meta;
mod query;
mod users;

use axum::{
    middleware,
    routing::{get, post},
    Router,
};
use std::sync::Arc;

use crate::{openapi, state::AppState};

/// 构建应用 Router。
pub fn build_router(state: Arc<AppState>) -> Router {
    // ---- 需要认证的路由（统一中间件） ----
    let protected = Router::new()
        .route("/auth/logout", post(auth::logout))
        .route("/auth/me", get(auth::me))
        .route("/auth/change-password", post(auth::change_password))
        .route("/users", get(users::list).post(users::create))
        .route(
            "/users/:id",
            get(users::detail).put(users::update).delete(users::delete),
        )
        .route("/users/:id/reset-password", post(users::reset_password))
        .route(
            "/users/:id/api-keys",
            get(apikeys::list_for_user).post(apikeys::create),
        )
        .route(
            "/api-keys/:key_id",
            axum::routing::put(apikeys::update).delete(apikeys::revoke),
        )
        // connections（D6/D7/D17；先注册静态段 /types 再注册参数段）
        .route("/connections/types", get(connections::types))
        .route(
            "/connections",
            get(connections::list).post(connections::create),
        )
        .route(
            "/connections/:id",
            get(connections::detail)
                .put(connections::update)
                .delete(connections::delete),
        )
        .route("/connections/:id/test", post(connections::test))
        .route("/connections/:id/lock", post(connections::lock))
        .route("/connections/:id/unlock", post(connections::unlock))
        // SQL 工作台 / 元数据 / 行编辑 / 导出（M2）
        .route("/query", post(query::execute))
        .route("/meta/databases", post(meta::databases))
        .route("/meta/tables", post(meta::tables))
        .route("/meta/columns", post(meta::columns))
        .route("/meta/ddl", post(meta::ddl))
        .route("/data/rows", post(data::list))
        .route("/data/rows/update", post(data::update))
        .route("/data/rows/delete", post(data::delete))
        .route("/export", post(export::create))
        .route("/export/download", get(export::download))
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
        // OpenAPI 契约（自产 openapi.json；Swagger UI 由客户端经 CDN 加载，后续迭代接入）
        .route("/api/v1/openapi.json", get(openapi::openapi_json_handler))
        .route("/api/v1/docs", get(docs_page))
        .nest("/api/v1", api)
        .fallback(not_found)
        .with_state(state)
}

/// GET /api/v1/docs —— 简化文档页（展示契约地址；正式 Swagger UI 后续接入）。
async fn docs_page() -> axum::response::Html<&'static str> {
    axum::response::Html(
        r#"<!doctype html><html lang="zh"><meta charset="utf-8">
<title>dbloom OpenAPI</title>
<body style="font-family:sans-serif;padding:2rem">
<h1>dbloom OpenAPI</h1>
<p>契约文档：<a href="/api/v1/openapi.json">/api/v1/openapi.json</a></p>
<p>完整 Swagger UI 将在后续迭代接入（CDN 方式，避免构建期外部下载）。</p>
</body></html>"#,
    )
}

async fn not_found() -> (axum::http::StatusCode, axum::Json<serde_json::Value>) {
    (
        axum::http::StatusCode::NOT_FOUND,
        axum::Json(serde_json::json!({
            "code": 40400,
            "message": "接口不存在（未匹配路由）",
            "trace_id": "",
        })),
    )
}
