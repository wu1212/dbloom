//! system 路由：health（02-api.md §2.8）。

use axum::{Json, extract::State};
use serde_json::json;
use sqlx::MySqlPool;
use std::sync::Arc;

use crate::state::AppState;

/// GET /api/v1/health
pub async fn health(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let storage = storage_ok(&state.pool).await;
    // 引擎真实探测（T5）：engine 活着=true；失败不拖垮 health，保持 200 + false + reason。
    let (engine, engine_reason) = match state.engine.ping_with_reason().await {
        Ok(summary) => (true, Some(summary)),
        Err(reason) => (false, Some(reason)),
    };
    Json(json!({
        "code": 0,
        "data": {
            "status": if storage { "ok" } else { "degraded" },
            "storage": storage,
            "engine": engine,
            "engine_reason": engine_reason,
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
