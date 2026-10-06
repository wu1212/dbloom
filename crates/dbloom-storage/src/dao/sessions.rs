//! sessions 表 DAO（refresh token 会话，可吊销，D19）。
//! 系统/管理员级资源（非租户私有）；只存 refresh token 的哈希。

use crate::error::StorageErrorExt;
use dbloom_common::AppError;
use sqlx::FromRow;
use sqlx::MySqlPool;

#[derive(Debug, Clone, FromRow)]
pub struct SessionRow {
    pub id: i64,
    pub user_id: i64,
    pub refresh_hash: String,
    pub expires_at: i64,
    pub revoke_reason: Option<String>,
    pub created_at: i64,
    pub last_used_at: Option<i64>,
}

#[derive(Clone)]
pub struct SessionDao {
    pool: MySqlPool,
}

impl SessionDao {
    pub fn new(pool: MySqlPool) -> Self {
        Self { pool }
    }

    pub async fn create(
        &self,
        user_id: i64,
        refresh_hash: &str,
        expires_at: i64,
        created_at: i64,
    ) -> Result<SessionRow, AppError> {
        sqlx::query_as::<_, SessionRow>(
            r#"
            INSERT INTO sessions (user_id, refresh_hash, expires_at, revoke_reason, created_at, last_used_at)
            VALUES (?, ?, ?, NULL, ?, NULL)
            "#,
        )
        .bind(user_id)
        .bind(refresh_hash)
        .bind(expires_at)
        .bind(created_at)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| e.storage_err())
    }

    pub async fn find_by_hash(&self, refresh_hash: &str) -> Result<SessionRow, AppError> {
        sqlx::query_as::<_, SessionRow>(
            "SELECT * FROM sessions WHERE refresh_hash = ? LIMIT 1",
        )
        .bind(refresh_hash)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| e.storage_err())
    }

    pub async fn update_last_used(&self, id: i64, at: i64) -> Result<(), AppError> {
        sqlx::query("UPDATE sessions SET last_used_at = ? WHERE id = ?")
            .bind(at)
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| e.storage_err())?;
        Ok(())
    }

    pub async fn revoke(&self, id: i64, reason: &str) -> Result<(), AppError> {
        sqlx::query("UPDATE sessions SET revoke_reason = ? WHERE id = ?")
            .bind(reason)
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| e.storage_err())?;
        Ok(())
    }

    /// 踢下线：吊销某用户全部未吊销会话（如强制重置密码/禁用账号）。
    pub async fn revoke_all_for_user(&self, user_id: i64, reason: &str) -> Result<(), AppError> {
        sqlx::query(
            "UPDATE sessions SET revoke_reason = ? WHERE user_id = ? AND revoke_reason IS NULL",
        )
        .bind(reason)
        .bind(user_id)
        .execute(&self.pool)
        .await
        .map_err(|e| e.storage_err())?;
        Ok(())
    }
}
