//! system 路由：health（02-api.md §2.8）。

use axum::{Json, extract::State};
use serde_json::json;
use sqlx::MySqlPool;
use std::sync::Arc;

use crate::state::AppState;

/// GET /api/v1/health
pub async fn health(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let storage = storage_ok(&state.pool).await;
    // 引擎连通（M3 接入后启用；D11 同主节点本地 8080）
    let engine = false;
    Json(json!({
        "code": 0,
        "data": {
            "status": if storage { "ok" } else { "degraded" },
            "storage": storage,
            "engine": engine,
            "version": env!("CARGO_PKG_VERSION"),
        }
    }))
}

async fn storage_ok(pool: &MySqlPool) -> bool {
    sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(pool)
        .await
        .is_ok()
}
