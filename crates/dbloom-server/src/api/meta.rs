//! 元数据读取路由（schema，02-api §2.6）。

use axum::{
    Extension, Json,
    extract::State,
};
use dbloom_common::AppError;
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

use crate::{
    auth::AuthCtx,
    error::{ok_json, ApiError},
    state::AppState,
};

/// 元数据请求体（连接 + 可选 database/table 参数）。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetaReq {
    pub connection_id: i64,
    #[serde(default)]
    pub database: Option<String>,
    #[serde(default)]
    pub table: Option<String>,
}

/// POST /api/v1/meta/databases
pub async fn databases(
    State(state): State<Arc<AppState>>,
    ctx: Extension<AuthCtx>,
    Json(req): Json<MetaReq>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let (_row, pool) = crate::api::query::open_pool_for(&state, &ctx, req.connection_id).await?;
    let dbs = dbloom_connector::databases(&pool)
        .await
        .map_err(|e| AppError::internal(format!("读取数据库列表失败: {e}")))?;
    Ok(ok_json(json!({ "items": dbs })))
}

/// POST /api/v1/meta/tables
pub async fn tables(
    State(state): State<Arc<AppState>>,
    ctx: Extension<AuthCtx>,
    Json(req): Json<MetaReq>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let (_row, pool) = crate::api::query::open_pool_for(&state, &ctx, req.connection_id).await?;
    let db = req
        .database
        .clone()
        .ok_or_else(|| ApiError::from(AppError::validation("缺少 database 参数")))?;
    let items = dbloom_connector::tables(&pool, &db)
        .await
        .map_err(|e| AppError::internal(format!("读取表列表失败: {e}")))?;
    Ok(ok_json(json!({ "database": db, "items": items })))
}

/// POST /api/v1/meta/columns
pub async fn columns(
    State(state): State<Arc<AppState>>,
    ctx: Extension<AuthCtx>,
    Json(req): Json<MetaReq>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let (_row, pool) = crate::api::query::open_pool_for(&state, &ctx, req.connection_id).await?;
    let db = req
        .database
        .clone()
        .ok_or_else(|| ApiError::from(AppError::validation("缺少 database 参数")))?;
    let table = req
        .table
        .clone()
        .ok_or_else(|| ApiError::from(AppError::validation("缺少 table 参数")))?;
    let items = dbloom_connector::columns(&pool, &db, &table)
        .await
        .map_err(|e| AppError::internal(format!("读取列定义失败: {e}")))?;
    Ok(ok_json(json!({ "database": db, "table": table, "items": items })))
}

/// POST /api/v1/meta/ddl
pub async fn ddl(
    State(state): State<Arc<AppState>>,
    ctx: Extension<AuthCtx>,
    Json(req): Json<MetaReq>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let (_row, pool) = crate::api::query::open_pool_for(&state, &ctx, req.connection_id).await?;
    let db = req
        .database
        .clone()
        .ok_or_else(|| ApiError::from(AppError::validation("缺少 database 参数")))?;
    let table = req
        .table
        .clone()
        .ok_or_else(|| ApiError::from(AppError::validation("缺少 table 参数")))?;
    let ddl = dbloom_connector::table_ddl(&pool, &db, &table)
        .await
        .map_err(|e| AppError::internal(format!("读取 DDL 失败: {e}")))?;
    Ok(ok_json(json!({ "database": db, "table": table, "ddl": ddl })))
}
