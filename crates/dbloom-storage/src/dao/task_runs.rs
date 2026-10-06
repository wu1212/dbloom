//! task_runs 表 DAO（`docs/design/01-data-model.md` §2 任务运行实例，D5/D22）。
//!
//! - `idempotency_key` 唯一：同一请求重复提交命中同一 run（T1 用例 7 幂等）。
//! - 租户过滤：`owner_user_id` 冗余列，普通用户仅见自己的 run；admin 可跨用户。

use dbloom_common::AppError;
use sqlx::{FromRow, MySqlPool};

use crate::error::StorageErrorExt;
use crate::tenant::TenantScope;

use super::normalize_page;

/// task_runs 表行。
#[derive(Debug, Clone, FromRow)]
pub struct TaskRunRow {
    pub id: i64,
    pub task_id: i64,
    pub owner_user_id: i64,
    pub trigger_type: String,
    pub idempotency_key: String,
    pub sea_tunnel_job_id: Option<String>,
    pub status: String,
    pub attempt: i32,
    pub start_time: Option<i64>,
    pub end_time: Option<i64>,
    pub error_message: Option<String>,
    pub log_path: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone)]
pub struct TaskRunDao {
    pool: MySqlPool,
}

impl TaskRunDao {
    pub fn new(pool: MySqlPool) -> Self {
        Self { pool }
    }

    /// 新建 run（pending）。`idempotency_key` 冲突时返回 None（命中既有 run）。
    /// 返回值：Some(新 run) 或 None（幂等命中既有 run，调用方应把既有 run 返回给客户端）。
    pub async fn create_pending(
        &self,
        task_id: i64,
        owner_user_id: i64,
        trigger_type: &str,
        idempotency_key: &str,
        attempt: i32,
        log_path: Option<&str>,
        now_ms: i64,
    ) -> Result<Option<TaskRunRow>, AppError> {
        let res = sqlx::query(
            r#"
            INSERT INTO task_runs
              (task_id, owner_user_id, trigger_type, idempotency_key, sea_tunnel_job_id,
               status, attempt, start_time, end_time, error_message, log_path,
               created_at, updated_at)
            VALUES (?, ?, ?, ?, NULL, 'pending', ?, NULL, NULL, NULL, ?, ?, ?)
            "#,
        )
        .bind(task_id)
        .bind(owner_user_id)
        .bind(trigger_type)
        .bind(idempotency_key)
        .bind(attempt)
        .bind(log_path)
        .bind(now_ms)
        .bind(now_ms)
        .execute(&self.pool)
        .await;

        match res {
            Ok(r) => {
                let id = r.last_insert_id() as i64;
                Ok(Some(self.get_by_id(id).await?))
            }
            Err(e) => {
                // 幂等命中：idempotency_key 唯一冲突 → 取既有 run 返回
                if e.to_string().contains("Duplicate entry") {
                    let _exist = sqlx::query_as::<_, TaskRunRow>(
                        "SELECT * FROM task_runs WHERE idempotency_key = ? LIMIT 1",
                    )
                    .bind(idempotency_key)
                    .fetch_one(&self.pool)
                    .await
                    .map_err(|e2| e2.storage_err())?;
                    Ok(None)
                } else {
                    Err(e.storage_err())
                }
            }
        }
    }

    pub async fn get_by_id(&self, id: i64) -> Result<TaskRunRow, AppError> {
        sqlx::query_as::<_, TaskRunRow>("SELECT * FROM task_runs WHERE id = ? LIMIT 1")
            .bind(id)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| e.storage_err())
    }

    /// 拿到 = 让 run 进入 running 并回填引擎 job id / 开始时间。
    pub async fn mark_running(
        &self,
        id: i64,
        job_id: &str,
        now_ms: i64,
    ) -> Result<(), AppError> {
        sqlx::query(
            "UPDATE task_runs SET status='running', sea_tunnel_job_id=?, start_time=IFNULL(start_time, ?), updated_at=? WHERE id=?",
        )
        .bind(job_id)
        .bind(now_ms)
        .bind(now_ms)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|e| e.storage_err())?;
        Ok(())
    }

    /// 回写终态（succeeded/failed/canceled/stopped），记错误信息与结束时间。
    pub async fn finish(
        &self,
        id: i64,
        status: &str,
        error_message: Option<&str>,
        now_ms: i64,
    ) -> Result<(), AppError> {
        sqlx::query(
            "UPDATE task_runs SET status=?, error_message=?, end_time=?, updated_at=? WHERE id=?",
        )
        .bind(status)
        .bind(error_message)
        .bind(now_ms)
        .bind(now_ms)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|e| e.storage_err())?;
        Ok(())
    }

    /// 按任务列出 run（分页，租户过滤；admin 可 ?user_id=）。
    pub async fn list_by_task(
        &self,
        scope: &TenantScope,
        task_id: i64,
        page: Option<i64>,
        page_size: Option<i64>,
    ) -> Result<(i64, Vec<TaskRunRow>), AppError> {
        let (page, size) = normalize_page(page, page_size);
        let offset = (page - 1) * size;
        let owner_filter: Option<i64> = if scope.can_see_all() {
            None
        } else {
            Some(scope.explicit_user_id.unwrap_or(scope.actor_user_id))
        };

        let mut where_sql = String::from("task_id = ?");
        if owner_filter.is_some() {
            where_sql.push_str(" AND owner_user_id = ?");
        }

        let count_sql = format!("SELECT COUNT(*) FROM task_runs WHERE {where_sql}");
        let mut total_q = sqlx::query_scalar::<_, i64>(&count_sql).bind(task_id);
        if let Some(o) = owner_filter {
            total_q = total_q.bind(o);
        }
        let total = total_q
            .fetch_one(&self.pool)
            .await
            .map_err(|e| e.storage_err())?;

        let list_sql = format!(
            "SELECT * FROM task_runs WHERE {where_sql} ORDER BY id DESC LIMIT ? OFFSET ?"
        );
        let mut q = sqlx::query_as::<_, TaskRunRow>(&list_sql).bind(task_id);
        if let Some(o) = owner_filter {
            q = q.bind(o);
        }
        let items = q
            .bind(size)
            .bind(offset)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| e.storage_err())?;

        Ok((total, items))
    }

    /// 某任务最近一次 run（用于 retry 定位最近失败 run）。
    pub async fn latest_for_task(&self, task_id: i64) -> Result<Option<TaskRunRow>, AppError> {
        let r = sqlx::query_as::<_, TaskRunRow>(
            "SELECT * FROM task_runs WHERE task_id = ? ORDER BY id DESC LIMIT 1",
        )
        .bind(task_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| e.storage_err())?;
        Ok(r)
    }

    /// 某任务当前 running run（stop 时定位运行中的 run）。
    pub async fn active_run(
        &self,
        scope: &TenantScope,
        task_id: i64,
    ) -> Result<Option<TaskRunRow>, AppError> {
        let owner_filter: Option<i64> = if scope.can_see_all() {
            None
        } else {
            Some(scope.explicit_user_id.unwrap_or(scope.actor_user_id))
        };
        let mut where_sql = String::from("task_id = ? AND status IN ('pending','running')");
        if owner_filter.is_some() {
            where_sql.push_str(" AND owner_user_id = ?");
        }
        let sql = format!("SELECT * FROM task_runs WHERE {where_sql} ORDER BY id DESC LIMIT 1");
        let mut f = sqlx::query_as::<_, TaskRunRow>(&sql).bind(task_id);
        if let Some(o) = owner_filter {
            f = f.bind(o);
        }
        let r = f.fetch_optional(&self.pool).await.map_err(|e| e.storage_err())?;
        Ok(r)
    }

    /// 统计某用户名下 run 数（admin 用量）。
    pub async fn count_by_owner(&self, owner: i64) -> Result<i64, AppError> {
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM task_runs WHERE owner_user_id = ?",
        )
        .bind(owner)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| e.storage_err())
    }
}
