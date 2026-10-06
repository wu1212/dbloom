//! tasks 表 DAO（`docs/design/01-data-model.md` §2 同步任务，D4/D5）。
//!
//! 租户规则（§3.4 + 02-api.md §1）：与 connections 相同——查询强制 `owner_user_id`
//! 注入（TenantScope）；单条读取由上层 handler 用 `can_access_owner` 做 404 判定。

use dbloom_common::AppError;
use sqlx::{FromRow, MySqlPool};

use crate::error::StorageErrorExt;
use crate::tenant::TenantScope;

use super::normalize_page;

/// tasks 表行（完整）。
#[derive(Debug, Clone, FromRow)]
pub struct TaskRow {
    pub id: i64,
    pub owner_user_id: i64,
    pub name: String,
    pub description: Option<String>,
    pub source_connection_id: i64,
    pub sink_connection_id: i64,
    pub sync_mode: String,
    /// 渲染后的 HOCON 快照（脱敏，不含密码）。
    pub config_hocon: String,
    /// 表映射（JSON 数组文本，如 `[{"sourceTable":"db.a","sinkTable":"db.b"}]`）。
    pub table_mapping: Option<String>,
    pub schedule_cron: Option<String>,
    pub enabled: bool,
    pub timeout_sec: i32,
    pub retry_times: i32,
    pub concurrency_limit: i32,
    pub created_by: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
    pub deleted_at: Option<i64>,
}

/// 更新字段集合（None=保持原值；config_hocon 重新渲染后整体替换）。
#[derive(Debug, Clone, Default)]
pub struct TaskUpdate {
    pub name: Option<String>,
    pub description: Option<Option<String>>,
    pub sync_mode: Option<String>,
    pub config_hocon: Option<String>,
    pub schedule_cron: Option<Option<String>>,
    pub enabled: Option<bool>,
    pub timeout_sec: Option<i32>,
    pub retry_times: Option<i32>,
}

#[derive(Clone)]
pub struct TaskDao {
    pool: MySqlPool,
}

impl TaskDao {
    pub fn new(pool: MySqlPool) -> Self {
        Self { pool }
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn create(
        &self,
        owner_user_id: i64,
        name: &str,
        description: Option<&str>,
        source_connection_id: i64,
        sink_connection_id: i64,
        sync_mode: &str,
        config_hocon: &str,
        table_mapping: Option<&str>, // JSON 数组文本
        schedule_cron: Option<&str>,
        enabled: bool,
        timeout_sec: i32,
        retry_times: i32,
        concurrency_limit: i32,
        created_by: Option<i64>,
        now_ms: i64,
    ) -> Result<TaskRow, AppError> {
        let res = sqlx::query(
            r#"
            INSERT INTO tasks
              (owner_user_id, name, description, source_connection_id, sink_connection_id,
               sync_mode, config_hocon, table_mapping, schedule_cron, enabled, timeout_sec, retry_times,
               concurrency_limit, created_by, created_at, updated_at, deleted_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, CAST(? AS JSON), ?, ?, ?, ?, ?, ?, ?, ?, NULL)
            "#,
        )
        .bind(owner_user_id)
        .bind(name)
        .bind(description)
        .bind(source_connection_id)
        .bind(sink_connection_id)
        .bind(sync_mode)
        .bind(config_hocon)
        .bind(table_mapping)
        .bind(schedule_cron)
        .bind(enabled)
        .bind(timeout_sec)
        .bind(retry_times)
        .bind(concurrency_limit)
        .bind(created_by)
        .bind(now_ms)
        .bind(now_ms)
        .execute(&self.pool)
        .await
        .map_err(|e| e.storage_err())?;
        let id = res.last_insert_id() as i64;
        self.get_by_id(id).await
    }

    /// 按 id 查未软删任务（不含租户条件，由上层做归属判定）。
    pub async fn get_by_id(&self, id: i64) -> Result<TaskRow, AppError> {
        sqlx::query_as::<_, TaskRow>(
            "SELECT * FROM tasks WHERE id = ? AND deleted_at IS NULL LIMIT 1",
        )
        .bind(id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| e.storage_err())
    }

    /// 列表（租户过滤注入；keyword 匹配 name；admin 可 ?user_id= 显式跨用户）。
    pub async fn list(
        &self,
        scope: &TenantScope,
        keyword: Option<&str>,
        sync_mode: Option<&str>,
        enabled: Option<bool>,
        page: Option<i64>,
        page_size: Option<i64>,
    ) -> Result<(i64, Vec<TaskRow>), AppError> {
        let (page, size) = normalize_page(page, page_size);
        let offset = (page - 1) * size;

        let kw = keyword.filter(|k| !k.is_empty()).map(|k| format!("%{k}%"));
        let sm = sync_mode.filter(|s| !s.is_empty()).map(|s| s.to_string());

        let mut where_sql = String::from("deleted_at IS NULL");
        let owner_filter: Option<i64> = if scope.can_see_all() {
            None
        } else {
            Some(scope.explicit_user_id.unwrap_or(scope.actor_user_id))
        };
        if owner_filter.is_some() {
            where_sql.push_str(" AND owner_user_id = ?");
        }
        if kw.is_some() {
            where_sql.push_str(" AND name LIKE ?");
        }
        if sm.is_some() {
            where_sql.push_str(" AND sync_mode = ?");
        }
        if let Some(en) = enabled {
            where_sql.push_str(if en { " AND enabled = 1" } else { " AND enabled = 0" });
        }

        let count_sql = format!("SELECT COUNT(*) FROM tasks WHERE {where_sql}");
        let total: i64 = {
            let mut q = sqlx::query_scalar::<_, i64>(&count_sql);
            if let Some(o) = owner_filter {
                q = q.bind(o);
            }
            if let Some(k) = kw.as_ref() {
                q = q.bind(k);
            }
            if let Some(s) = sm.as_ref() {
                q = q.bind(s);
            }
            q.fetch_one(&self.pool).await.map_err(|e| e.storage_err())?
        };

        let list_sql = format!(
            "SELECT * FROM tasks WHERE {where_sql} ORDER BY id DESC LIMIT ? OFFSET ?"
        );
        let mut q = sqlx::query_as::<_, TaskRow>(&list_sql);
        if let Some(o) = owner_filter {
            q = q.bind(o);
        }
        if let Some(k) = kw.as_ref() {
            q = q.bind(k);
        }
        if let Some(s) = sm.as_ref() {
            q = q.bind(s);
        }
        let items = q
            .bind(size)
            .bind(offset)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| e.storage_err())?;

        Ok((total, items))
    }

    /// 更新（读原行 → 合并变更 → 全列 UPDATE）。
    pub async fn update(&self, id: i64, u: &TaskUpdate, now_ms: i64) -> Result<TaskRow, AppError> {
        let row = self.get_by_id(id).await?;
        let name = u.name.clone().unwrap_or(row.name);
        let description = u
            .description
            .as_ref()
            .map(|v| v.clone())
            .unwrap_or(row.description);
        let sync_mode = u.sync_mode.clone().unwrap_or(row.sync_mode);
        let config_hocon = u.config_hocon.clone().unwrap_or(row.config_hocon);
        let schedule_cron = u
            .schedule_cron
            .as_ref()
            .map(|v| v.clone())
            .unwrap_or(row.schedule_cron);
        let enabled = u.enabled.unwrap_or(row.enabled);
        let timeout_sec = u.timeout_sec.unwrap_or(row.timeout_sec);
        let retry_times = u.retry_times.unwrap_or(row.retry_times);

        sqlx::query(
            r#"
            UPDATE tasks SET
              name=?, description=?, sync_mode=?, config_hocon=?, schedule_cron=?,
              enabled=?, timeout_sec=?, retry_times=?, updated_at=?
            WHERE id=?
            "#,
        )
        .bind(name)
        .bind(description)
        .bind(sync_mode)
        .bind(config_hocon)
        .bind(schedule_cron)
        .bind(enabled)
        .bind(timeout_sec)
        .bind(retry_times)
        .bind(now_ms)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|e| e.storage_err())?;
        self.get_by_id(id).await
    }

    /// 切换调度开关。
    pub async fn set_enabled(&self, id: i64, enabled: bool, now_ms: i64) -> Result<(), AppError> {
        sqlx::query("UPDATE tasks SET enabled = ?, updated_at = ? WHERE id = ?")
            .bind(enabled)
            .bind(now_ms)
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| e.storage_err())?;
        Ok(())
    }

    /// 软删任务（先停再删交给上层：本方法只置 deleted_at）。
    pub async fn soft_delete(&self, id: i64, now_ms: i64) -> Result<(), AppError> {
        sqlx::query("UPDATE tasks SET deleted_at = ?, updated_at = ? WHERE id = ?")
            .bind(now_ms)
            .bind(now_ms)
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| e.storage_err())?;
        Ok(())
    }

    /// 统计某用户名下任务数（admin 用户详情用量）。
    pub async fn count_by_owner(&self, owner: i64) -> Result<i64, AppError> {
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM tasks WHERE owner_user_id = ? AND deleted_at IS NULL",
        )
        .bind(owner)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| e.storage_err())
    }
}
