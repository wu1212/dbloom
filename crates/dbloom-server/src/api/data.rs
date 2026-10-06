//! 行级数据浏览 / 编辑 / 删除（data，02-api §2.7，D6 写保护）。

use axum::{
    Extension, Json,
    extract::State,
};
use dbloom_common::AppError;
use dbloom_connector::SqlKind;
use dbloom_types::{RowDeleteRequest, RowListRequest, RowUpdateRequest, RowWriteResult};
use serde_json::json;
use std::sync::Arc;

use crate::{
    auth::AuthCtx,
    error::{ok_json, ApiError},
    state::AppState,
};

/// POST /api/v1/data/rows —— 表数据浏览（分页）。
pub async fn list(
    State(state): State<Arc<AppState>>,
    ctx: Extension<AuthCtx>,
    Json(req): Json<RowListRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let (_row, pool) = crate::api::query::open_pool_for(&state, &ctx, req.connection_id).await?;
    let (cols, rows, total) = dbloom_connector::list_rows(
        &pool,
        &req.table,
        req.page.unwrap_or(1),
        req.page_size.unwrap_or(100),
    )
    .await
    .map_err(|e| ApiError::from(AppError::internal(format!("浏览数据失败: {e}"))))?;
    Ok(ok_json(json!({ "columns": cols, "rows": rows, "total": total })))
}

/// 行写操作公共守卫：只读锁一律拒；未确认（confirm!=true）返回 needConfirm。
async fn row_write_guard(
    state: &AppState,
    ctx: &AuthCtx,
    conn_id: i64,
    confirm: bool,
) -> Result<(), ApiError> {
    let (row, _pool) = crate::api::query::open_pool_for(state, ctx, conn_id).await?;
    if let Some(reason) = crate::api::query::write_guard_reason(&row, &SqlKind::Write, confirm) {
        return Err(ApiError::from(AppError::validation(reason)));
    }
    Ok(())
}

/// POST /api/v1/data/rows/update —— 更新指定行（D6 需确认）。
pub async fn update(
    State(state): State<Arc<AppState>>,
    ctx: Extension<AuthCtx>,
    Json(req): Json<RowUpdateRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if req.values.is_empty() || req.keys.is_empty() {
        return Err(ApiError::from(AppError::validation("更新行需要 values 与 keys（WHERE 主键）")));
    }
    row_write_guard(&state, &ctx, req.connection_id, req.confirm.unwrap_or(false)).await?;
    let (_row, pool) = crate::api::query::open_pool_for(&state, &ctx, req.connection_id).await?;
    let affected = dbloom_connector::update_row(&pool, &req.table, &req.keys, &req.values)
        .await
        .map_err(|e| ApiError::from(AppError::internal(format!("更新失败: {e}"))))?;
    state
        .iam
        .record_audit(
            Some(ctx.user_id()),
            "user",
            "row_update",
            Some("data"),
            Some(&req.connection_id.to_string()),
            Some(json!({ "table": req.table, "affected": affected })),
            None,
        )
        .await
        .map_err(ApiError::from)?;
    Ok(ok_json(RowWriteResult {
        success: true,
        affected_rows: affected,
        need_confirm: None,
        confirmed: Some(true),
    }))
}

/// POST /api/v1/data/rows/delete —— 删除指定行（D6 需确认）。
pub async fn delete(
    State(state): State<Arc<AppState>>,
    ctx: Extension<AuthCtx>,
    Json(req): Json<RowDeleteRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if req.keys.is_empty() {
        return Err(ApiError::from(AppError::validation("删除行需要 keys（WHERE 主键）")));
    }
    row_write_guard(&state, &ctx, req.connection_id, req.confirm.unwrap_or(false)).await?;
    let (_row, pool) = crate::api::query::open_pool_for(&state, &ctx, req.connection_id).await?;
    let affected = dbloom_connector::delete_row(&pool, &req.table, &req.keys)
        .await
        .map_err(|e| ApiError::from(AppError::internal(format!("删除失败: {e}"))))?;
    state
        .iam
        .record_audit(
            Some(ctx.user_id()),
            "user",
            "row_delete",
            Some("data"),
            Some(&req.connection_id.to_string()),
            Some(json!({ "table": req.table, "affected": affected })),
            None,
        )
        .await
        .map_err(ApiError::from)?;
    Ok(ok_json(RowWriteResult {
        success: true,
        affected_rows: affected,
        need_confirm: None,
        confirmed: Some(true),
    }))
}
