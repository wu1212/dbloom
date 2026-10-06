//! audit_logs 表 DAO（D21：审计；只存脱敏摘要）。

use crate::error::StorageErrorExt;
use dbloom_common::AppError;
use serde_json::Value;
use sqlx::FromRow;
use sqlx::MySqlPool;

#[derive(Debug, Clone, FromRow)]
pub struct AuditRow {
    pub id: i64,
    pub actor_user_id: Option<i64>,
    pub actor_type: String,
    pub action: String,
    pub resource_type: Option<String>,
    pub resource_id: Option<String>,
    pub detail_json: Option<Value>,
    pub ip: Option<String>,
    pub created_at: i64,
}

#[derive(Clone)]
pub struct AuditDao {
    pool: MySqlPool,
}

impl AuditDao {
    pub fn new(pool: MySqlPool) -> Self {
        Self { pool }
    }

    /// 写一条审计记录。detail 只存摘要（`01-data-model.md` §3.3）。
    #[allow(clippy::too_many_arguments)]
    pub async fn insert(
        &self,
        actor_user_id: Option<i64>,
        actor_type: &str,
        action: &str,
        resource_type: Option<&str>,
        resource_id: Option<&str>,
        detail_json: Option<Value>,
        ip: Option<&str>,
        created_at: i64,
    ) -> Result<AuditRow, AppError> {
        sqlx::query_as::<_, AuditRow>(
            r#"
            INSERT INTO audit_logs
              (actor_user_id, actor_type, action, resource_type, resource_id, detail_json, ip, created_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(actor_user_id)
        .bind(actor_type)
        .bind(action)
        .bind(resource_type)
        .bind(resource_id)
        .bind(detail_json)
        .bind(ip)
        .bind(created_at)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| e.storage_err())
    }

    /// 按 actor 与动作分页查询（admin 域，M0 提供雏形）。
    #[allow(dead_code)]
    pub async fn list(
        &self,
        actor_user_id: Option<i64>,
        action: Option<&str>,
        page: Option<i64>,
        page_size: Option<i64>,
    ) -> Result<(i64, Vec<AuditRow>), AppError> {
        let (page, size) = super::normalize_page(page, page_size);
        let offset = (page - 1) * size;
        let act = action.filter(|a| !a.is_empty()).map(|a| a.to_string());

        let mut where_sql = String::from("1=1");
        if actor_user_id.is_some() {
            where_sql.push_str(" AND actor_user_id = ?");
        }
        if act.is_some() {
            where_sql.push_str(" AND action = ?");
        }
        let count_sql = format!("SELECT COUNT(*) FROM audit_logs WHERE {where_sql}");
        let total: i64 = {
            let mut q = sqlx::query_scalar::<_, i64>(&count_sql);
            if let Some(id) = actor_user_id {
                q = q.bind(id);
            }
            if let Some(a) = act.as_ref() {
                q = q.bind(a);
            }
            q.fetch_one(&self.pool).await.map_err(|e| e.storage_err())?
        };
        let list_sql = format!(
            "SELECT * FROM audit_logs WHERE {where_sql} ORDER BY id DESC LIMIT ? OFFSET ?"
        );
        let mut q = sqlx::query_as::<_, AuditRow>(&list_sql);
        if let Some(id) = actor_user_id {
            q = q.bind(id);
        }
        if let Some(a) = act.as_ref() {
            q = q.bind(a);
        }
        let items = q
            .bind(size)
            .bind(offset)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| e.storage_err())?;
        Ok((total, items))
    }
}
