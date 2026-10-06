//! SQL 工作台：执行 / 取消（超时）/ 分页 / D6 写保护（02-api §2.5）。
//!
//! 写保护链路（D6，04-security §2.3）：
//! - `classify_sql` 判定 `Read / Write / Danger`；
//! - 连接 `read_only_lock=true` → 一切写操作直接拒绝（403），无需确认；
//! - 写/危险语句默认拦截，返回 `needConfirm`（200 业务结果）；携带 `confirm=true` 重发后执行；
//! - 查询超时默认 120s（connector 层 tokio timeout），前端可放弃等待（连接不再复用）。

use axum::{
    Extension, Json,
    extract::State,
};
use dbloom_common::{AppError, time::now_ms};
use dbloom_connector::{SqlKind, classify_sql, exec_write, open_pool, query_rows};
use dbloom_storage::dao::ConnectionRow;
use dbloom_types::{QueryRequest, QueryResult};
use serde_json::json;
use std::sync::Arc;

use crate::{
    auth::AuthCtx,
    error::{ok_json, ApiError},
    state::AppState,
};

/// 从已保存连接行构建连接参数（解密凭据，仅进程内存，不落日志/响应）。
pub(crate) fn conn_params_from_row(
    state: &AppState,
    row: &ConnectionRow,
) -> Result<dbloom_connector::ConnParams, ApiError> {
    let password = match row.password_enc.as_deref() {
        Some(enc) => Some(state.crypto.decrypt_str(enc).map_err(ApiError::from)?),
        None => None,
    };
    Ok(dbloom_connector::ConnParams {
        conn_type: row.conn_type.clone(),
        host: row.host.clone(),
        port: row.port.map(|p| p as u16),
        database: row.database_name.clone(),
        username: row.username.clone(),
        password,
        ssl_mode: row.ssl_mode.clone(),
    })
}

/// 加载可见连接 + 打开连接池（M2：每次请求建池；连接池缓存列为后续优化）。
pub(crate) async fn open_pool_for(
    state: &AppState,
    ctx: &AuthCtx,
    conn_id: i64,
) -> Result<(ConnectionRow, dbloom_connector::DbPool), ApiError> {
    let row = crate::api::connections::load_visible(state, ctx, conn_id).await?;
    let params = conn_params_from_row(state, &row)?;
    let pool = open_pool(&params)
        .await
        .map_err(|e| AppError::internal(format!("无法连接数据源: {e}")))?;
    Ok((row, pool))
}

/// D6 写保护判定：返回拦截原因（None=放行）。
pub(crate) fn write_guard_reason(row: &ConnectionRow, kind: &SqlKind, confirm: bool) -> Option<String> {
    if *kind == SqlKind::Read {
        return None;
    }
    if row.read_only_lock {
        return Some("连接已启用只读锁（readOnlyLock），禁止任何写操作".to_string());
    }
    if !confirm {
        return Some(match kind {
            SqlKind::Danger => "高危操作，请确认后携带 confirm=true 重发".to_string(),
            _ => "写操作需要二次确认，请确认后携带 confirm=true 重发".to_string(),
        });
    }
    None
}

/// 空结果构造（写保护拦截 / 无结果集时复用）。
fn empty_result(conn_err: Option<String>) -> QueryResult {
    QueryResult {
        success: conn_err.is_none(),
        columns: vec![],
        rows: vec![],
        total: 0,
        page: 1,
        page_size: 0,
        has_more: false,
        exec_ms: 0,
        affected_rows: None,
        need_confirm: conn_err,
        confirmed: None,
    }
}

/// POST /api/v1/query —— 执行 SQL（读分页 / 写需确认）。
pub async fn execute(
    State(state): State<Arc<AppState>>,
    ctx: Extension<AuthCtx>,
    Json(req): Json<QueryRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let sql = req.sql.trim();
    if sql.is_empty() {
        return Err(ApiError::from(AppError::validation("SQL 不能为空")));
    }
    let (row, pool) = open_pool_for(&state, &ctx, req.connection_id).await?;
    let timeout = req.timeout_ms.unwrap_or(120_000).clamp(1000, 120_000);
    let (kind, _) = classify_sql(sql);

    // D6：只读锁 / 二次确认拦截
    if let Some(reason) = write_guard_reason(&row, &kind, req.confirm.unwrap_or(false)) {
        state
            .iam
            .record_audit(
                Some(ctx.user_id()),
                "user",
                "query_blocked",
                Some("query"),
                Some(&req.connection_id.to_string()),
                Some(json!({ "kind": format!("{kind:?}"), "sql": truncate(sql, 200), "reason": reason })),
                None,
            )
            .await
            .map_err(ApiError::from)?;
        return Ok(ok_json(empty_result(Some(reason))));
    }

    let start = now_ms();
    let kind_name = format!("{kind:?}");
    if kind == SqlKind::Read {
        let r = query_rows(
            &pool,
            sql,
            req.page.unwrap_or(1),
            req.page_size.unwrap_or(100),
            timeout,
        )
        .await
        .map_err(|e| ApiError::from(AppError::internal(format!("执行查询失败: {e}"))))?;
        state
            .iam
            .record_audit(
                Some(ctx.user_id()),
                "user",
                "query_exec",
                Some("query"),
                Some(&req.connection_id.to_string()),
                Some(json!({ "kind": kind_name, "sql": truncate(sql, 120), "rows": r.rows.len(), "ms": now_ms() - start })),
                None,
            )
            .await
            .map_err(ApiError::from)?;
        Ok(ok_json(r))
    } else {
        let affected = exec_write(&pool, sql, timeout)
            .await
            .map_err(|e| ApiError::from(AppError::internal(format!("执行写语句失败: {e}"))))?;
        state
            .iam
            .record_audit(
                Some(ctx.user_id()),
                "user",
                "query_write",
                Some("query"),
                Some(&req.connection_id.to_string()),
                Some(json!({ "kind": kind_name, "sql": truncate(sql, 120), "affected": affected, "ms": now_ms() - start })),
                None,
            )
            .await
            .map_err(ApiError::from)?;
        let mut res = empty_result(None);
        res.affected_rows = Some(affected);
        res.confirmed = Some(true);
        res.exec_ms = now_ms() - start;
        Ok(ok_json(res))
    }
}

/// 审计日志里截断 SQL（防把密钥/超长语句刷进日志）。
pub(crate) fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let t: String = s.chars().take(max).collect();
        format!("{t}…")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dbloom_connector::{SqlKind, classify_sql};
    use dbloom_storage::dao::ConnectionRow;

    /// 构造最小连接行（只关心隔离判定字段）。
    fn row(locked: bool) -> ConnectionRow {
        ConnectionRow {
            id: 1,
            owner_user_id: 1,
            name: "t".into(),
            conn_type: "mysql".into(),
            host: "h".into(),
            port: Some(3306),
            database_name: Some("d".into()),
            username: Some("u".into()),
            password_enc: None,
            ssl_mode: "disable".into(),
            extra_params: None,
            is_production: false,
            read_only_lock: locked,
            created_at: 0,
            updated_at: 0,
            deleted_at: None,
        }
    }

    #[test]
    fn readonly_lock_blocks_all_writes() {
        let locked = row(true);
        // 只读锁：Read 不拦，但 Update/Delete/Insert 一律拒（无论 confirm）
        assert_eq!(write_guard_reason(&locked, &SqlKind::Read, true), None);
        for (kind, confirm) in [
            (SqlKind::Write, true),
            (SqlKind::Write, false),
            (SqlKind::Danger, true),
            (SqlKind::Danger, false),
        ] {
            let r = write_guard_reason(&locked, &kind, confirm);
            assert!(r.is_some(), "锁连接上 {kind:?} confirm={confirm} 应被拒");
            assert!(r.unwrap().contains("只读锁"));
        }
    }

    #[test]
    fn write_without_confirm_asks_confirmation() {
        let unlocked = row(false);
        let r = write_guard_reason(&unlocked, &SqlKind::Write, false);
        assert!(r.is_some());
        assert!(r.unwrap().contains("confirm"));
    }

    #[test]
    fn danger_without_confirm_asks_confirmation() {
        let unlocked = row(false);
        let r = write_guard_reason(&unlocked, &SqlKind::Danger, false);
        assert!(r.is_some());
        assert!(r.unwrap().contains("confirm"));
    }

    #[test]
    fn write_with_confirm_passes_on_unlocked() {
        let unlocked = row(false);
        assert_eq!(write_guard_reason(&unlocked, &SqlKind::Write, true), None);
        assert_eq!(write_guard_reason(&unlocked, &SqlKind::Danger, true), None);
        assert_eq!(write_guard_reason(&unlocked, &SqlKind::Read, false), None);
    }

    #[test]
    fn classify_and_guard_end_to_end() {
        // UPDATE 无 WHERE → Danger → 需 confirm
        let (k, _) = classify_sql("UPDATE t SET a=1");
        assert_eq!(k, SqlKind::Danger);
        assert!(write_guard_reason(&row(false), &k, false).is_some());
        // UPDATE 带 WHERE → Write → 需 confirm
        let (k2, _) = classify_sql("UPDATE t SET a=1 WHERE id=2");
        assert_eq!(k2, SqlKind::Write);
        assert!(write_guard_reason(&row(false), &k2, false).is_some());
        assert_eq!(write_guard_reason(&row(false), &k2, true), None);
        // 多语句 → Danger → 锁上拒
        let (k3, _) = classify_sql("SELECT 1; DROP TABLE t");
        assert_eq!(k3, SqlKind::Danger);
    }
}
