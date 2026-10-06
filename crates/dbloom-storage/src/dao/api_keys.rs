//! api_keys 表 DAO（D9：仅管理员签发/管理）。
//! 只存密钥哈希（SHA-256），明文仅在签发响应返回一次。

use crate::error::StorageErrorExt;
use dbloom_common::AppError;
use sqlx::FromRow;
use sqlx::MySqlPool;

#[derive(Debug, Clone, FromRow)]
pub struct ApiKeyRow {
    pub id: i64,
    pub user_id: i64,
    pub created_by: i64,
    pub name: String,
    pub key_hash: String,
    pub prefix: String,
    pub status: String,
    pub valid_from: Option<i64>,
    pub valid_until: Option<i64>,
    pub last_used_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
    pub deleted_at: Option<i64>,
}

#[derive(Clone)]
pub struct ApiKeyDao {
    pool: MySqlPool,
}

impl ApiKeyDao {
    pub fn new(pool: MySqlPool) -> Self {
        Self { pool }
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn create(
        &self,
        user_id: i64,
        created_by: i64,
        name: &str,
        key_hash: &str,
        prefix: &str,
        valid_from: Option<i64>,
        valid_until: Option<i64>,
        now_ms: i64,
    ) -> Result<ApiKeyRow, AppError> {
        let res = sqlx::query(
            r#"
            INSERT INTO api_keys
              (user_id, created_by, name, key_hash, prefix, status,
               valid_from, valid_until, last_used_at, created_at, updated_at, deleted_at)
            VALUES (?, ?, ?, ?, ?, 'enabled', ?, ?, NULL, ?, ?, NULL)
            "#,
        )
        .bind(user_id)
        .bind(created_by)
        .bind(name)
        .bind(key_hash)
        .bind(prefix)
        .bind(valid_from)
        .bind(valid_until)
        .bind(now_ms)
        .bind(now_ms)
        .execute(&self.pool)
        .await
        .map_err(|e| e.storage_err())?;
        let id = res.last_insert_id() as i64;
        self.get_by_id(id).await
    }

    pub async fn get_by_id(&self, id: i64) -> Result<ApiKeyRow, AppError> {
        sqlx::query_as::<_, ApiKeyRow>(
            "SELECT * FROM api_keys WHERE id = ? AND deleted_at IS NULL LIMIT 1",
        )
        .bind(id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| e.storage_err())
    }

    pub async fn get_by_hash(&self, key_hash: &str) -> Result<ApiKeyRow, AppError> {
        sqlx::query_as::<_, ApiKeyRow>(
            "SELECT * FROM api_keys WHERE key_hash = ? AND deleted_at IS NULL LIMIT 1",
        )
        .bind(key_hash)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| e.storage_err())
    }

    pub async fn list_by_user(&self, user_id: i64) -> Result<Vec<ApiKeyRow>, AppError> {
        sqlx::query_as::<_, ApiKeyRow>(
            "SELECT * FROM api_keys WHERE user_id = ? AND deleted_at IS NULL ORDER BY id DESC",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| e.storage_err())
    }

    pub async fn list_all(&self) -> Result<Vec<ApiKeyRow>, AppError> {
        sqlx::query_as::<_, ApiKeyRow>(
            "SELECT * FROM api_keys WHERE deleted_at IS NULL ORDER BY id DESC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| e.storage_err())
    }

    /// 更新 name / status / 生效 / 失效时间（D9 生命周期）。
    pub async fn update_fields(
        &self,
        id: i64,
        name: Option<&str>,
        status: Option<&str>,
        valid_from: Option<Option<i64>>,
        valid_until: Option<Option<i64>>,
        now_ms: i64,
    ) -> Result<(), AppError> {
        let row = self.get_by_id(id).await?;
        let final_name = name.map(|s| s.to_string()).unwrap_or(row.name);
        let final_status = status.map(|s| s.to_string()).unwrap_or(row.status);
        sqlx::query(
            "UPDATE api_keys SET name = ?, status = ?, valid_from = ?, valid_until = ?, updated_at = ? WHERE id = ?",
        )
        .bind(final_name)
        .bind(final_status)
        .bind(valid_from.unwrap_or(row.valid_from))
        .bind(valid_until.unwrap_or(row.valid_until))
        .bind(now_ms)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|e| e.storage_err())?;
        Ok(())
    }

    /// 撤销（软删，立即全局生效，D9）。
    pub async fn soft_delete(&self, id: i64, now_ms: i64) -> Result<(), AppError> {
        sqlx::query("UPDATE api_keys SET deleted_at = ?, updated_at = ? WHERE id = ? AND deleted_at IS NULL")
            .bind(now_ms)
            .bind(now_ms)
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| e.storage_err())?;
        Ok(())
    }

    pub async fn touch_last_used(&self, id: i64, at: i64) -> Result<(), AppError> {
        sqlx::query("UPDATE api_keys SET last_used_at = ? WHERE id = ?")
            .bind(at)
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| e.storage_err())?;
        Ok(())
    }
}
