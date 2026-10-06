//! audit-logs 路由（02-api §2.8，D21：全量审计查询，仅 admin）。

use axum::{
    Extension, Json,
    extract::{Query, State},
};
use dbloom_common::AppError;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::{
    auth::AuthCtx,
    error::{ok_json, ApiError},
    state::AppState,
};

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditListQuery {
    pub user_id: Option<i64>,
    pub action: Option<String>,
    pub page: Option<i64>,
    pub page_size: Option<i64>,
}

/// GET /api/v1/audit-logs（admin only）
pub async fn list(
    State(state): State<Arc<AppState>>,
    ctx: Extension<AuthCtx>,
    Query(q): Query<AuditListQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if !ctx.is_admin() {
        return Err(ApiError::from(AppError::forbidden("仅管理员可查看审计日志")));
    }
    let (total, rows) = state
        .iam
        .audit
        .list(q.user_id, q.action.as_deref(), q.page, q.page_size)
        .await
        .map_err(ApiError::from)?;
    let items: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "id": r.id,
                "actorUserId": r.actor_user_id,
                "actorType": r.actor_type,
                "action": r.action,
                "resourceType": r.resource_type,
                "resourceId": r.resource_id,
                "detail": r.detail_json,
                "ip": r.ip,
                "createdAt": r.created_at,
            })
        })
        .collect();
    Ok(ok_json(serde_json::json!({ "total": total, "items": items })))
}
