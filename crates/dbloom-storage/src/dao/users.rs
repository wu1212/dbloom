//! users 表 DAO（`docs/design/01-data-model.md` §2）。

use dbloom_common::AppError;
use sqlx::FromRow;
use sqlx::MySqlPool;

use crate::error::StorageErrorExt;

use super::normalize_page;

#[derive(Debug, Clone, FromRow)]
pub struct UserRow {
    pub id: i64,
    pub username: String,
    pub password_hash: String,
    pub role: String,
    pub status: String,
    pub display_name: Option<String>,
    pub must_change_password: bool,
    pub failed_login_count: i32,
    pub locked_until: Option<i64>,
    pub created_by: Option<i64>,
    pub last_login_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
    pub deleted_at: Option<i64>,
}

#[derive(Clone)]
pub struct UserDao {
    pool: MySqlPool,
}

impl UserDao {
    pub fn new(pool: MySqlPool) -> Self {
        Self { pool }
    }

    pub async fn create(
        &self,
        username: &str,
        password_hash: &str,
        role: &str,
        created_by: Option<i64>,
        now_ms: i64,
    ) -> Result<UserRow, AppError> {
        let res = sqlx::query(
            r#"
            INSERT INTO users
              (username, password_hash, role, status, display_name, must_change_password,
               failed_login_count, locked_until, created_by, last_login_at,
               created_at, updated_at, deleted_at)
            VALUES (?, ?, ?, 'active', NULL, 1, 0, NULL, ?, NULL, ?, ?, NULL)
            "#,
        )
        .bind(username)
        .bind(password_hash)
        .bind(role)
        .bind(created_by)
        .bind(now_ms)
        .bind(now_ms)
        .execute(&self.pool)
        .await
        .map_err(|e| e.storage_err())?;
        let id = res.last_insert_id() as i64;
        self.get_by_id(id).await
    }

    /// 按 id 查未软删用户。
    pub async fn get_by_id(&self, id: i64) -> Result<UserRow, AppError> {
        sqlx::query_as::<_, UserRow>(
            "SELECT * FROM users WHERE id = ? AND deleted_at IS NULL LIMIT 1",
        )
        .bind(id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| e.storage_err())
    }

    /// 按用户名查未软删用户。
    pub async fn get_by_username(&self, username: &str) -> Result<UserRow, AppError> {
        sqlx::query_as::<_, UserRow>(
            "SELECT * FROM users WHERE username = ? AND deleted_at IS NULL LIMIT 1",
        )
        .bind(username)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| e.storage_err())
    }

    /// 分页列表（admin 域）；keyword 模糊匹配 username/display_name，status 精确。
    pub async fn list(
        &self,
        keyword: Option<&str>,
        status: Option<&str>,
        page: Option<i64>,
        page_size: Option<i64>,
    ) -> Result<(i64, Vec<UserRow>), AppError> {
        let (page, size) = normalize_page(page, page_size);
        let offset = (page - 1) * size;

        // 把过滤参数提升到函数作用域，避免 bind 借用悬垂。
        let kw = keyword
            .filter(|k| !k.is_empty())
            .map(|k| format!("%{k}%"));
        let st = status.filter(|s| !s.is_empty()).map(|s| s.to_string());

        let mut where_sql = String::from("deleted_at IS NULL");
        if kw.is_some() {
            where_sql.push_str(" AND (username LIKE ? OR display_name LIKE ?)");
        }
        if st.is_some() {
            where_sql.push_str(" AND status = ?");
        }

        let count_sql = format!("SELECT COUNT(*) FROM users WHERE {where_sql}");
        let total: i64 = {
            let mut q = sqlx::query_scalar::<_, i64>(&count_sql);
            if let Some(k) = kw.as_ref() {
                q = q.bind(k).bind(k);
            }
            if let Some(s) = st.as_ref() {
                q = q.bind(s);
            }
            q.fetch_one(&self.pool)
                .await
                .map_err(|e| e.storage_err())?
        };

        let list_sql = format!(
            "SELECT * FROM users WHERE {where_sql} ORDER BY id DESC LIMIT ? OFFSET ?"
        );
        let mut q = sqlx::query_as::<_, UserRow>(&list_sql);
        if let Some(k) = kw.as_ref() {
            q = q.bind(k).bind(k);
        }
        if let Some(s) = st.as_ref() {
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

    /// 更新显示名与状态（admin 域）。
    pub async fn update_profile(
        &self,
        id: i64,
        display_name: Option<&str>,
        status: Option<&str>,
        now_ms: i64,
    ) -> Result<(), AppError> {
        let row = self.get_by_id(id).await?;
        let final_name = display_name.map(|s| s.to_string()).or(row.display_name);
        let final_status = status.map(|s| s.to_string()).unwrap_or(row.status);
        sqlx::query(
            "UPDATE users SET display_name = ?, status = ?, updated_at = ? WHERE id = ?",
        )
        .bind(final_name)
        .bind(final_status)
        .bind(now_ms)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|e| e.storage_err())?;
        Ok(())
    }

    /// 设置密码哈希与强制改密标记（首登/被重置，D20）。
    pub async fn set_password(
        &self,
        id: i64,
        new_hash: &str,
        must_change_password: bool,
        now_ms: i64,
    ) -> Result<(), AppError> {
        sqlx::query(
            "UPDATE users SET password_hash = ?, must_change_password = ?, updated_at = ?, failed_login_count = 0, locked_until = NULL WHERE id = ?",
        )
        .bind(new_hash)
        .bind(must_change_password)
        .bind(now_ms)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|e| e.storage_err())?;
        Ok(())
    }

    pub async fn set_last_login(&self, id: i64, at_ms: i64) -> Result<(), AppError> {
        sqlx::query("UPDATE users SET last_login_at = ? WHERE id = ?")
            .bind(at_ms)
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| e.storage_err())?;
        Ok(())
    }

    /// 登录失败：累加计数（D19），返回更新后的计数。
    pub async fn inc_failed_login(&self, id: i64, now_ms: i64) -> Result<i32, AppError> {
        sqlx::query(
            "UPDATE users SET failed_login_count = failed_login_count + 1, updated_at = ? WHERE id = ?",
        )
        .bind(now_ms)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|e| e.storage_err())?;
        let r = self.get_by_id(id).await?;
        Ok(r.failed_login_count)
    }

    pub async fn clear_failed_login(&self, id: i64, now_ms: i64) -> Result<(), AppError> {
        sqlx::query("UPDATE users SET failed_login_count = 0, updated_at = ? WHERE id = ?")
            .bind(now_ms)
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| e.storage_err())?;
        Ok(())
    }

    /// 锁定到指定时刻（D19：5 次失败/5min）。
    pub async fn set_locked(&self, id: i64, locked_until: Option<i64>, now_ms: i64) -> Result<(), AppError> {
        sqlx::query("UPDATE users SET locked_until = ?, updated_at = ? WHERE id = ?")
            .bind(locked_until)
            .bind(now_ms)
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| e.storage_err())?;
        Ok(())
    }

    /// 软删 + 用户名匿名化（`docs/design/01-data-model.md` §5）。
    pub async fn soft_delete(&self, id: i64, now_ms: i64) -> Result<(), AppError> {
        let row = self.get_by_id(id).await?;
        let anonymized = format!("{}__deleted_{}", row.username, row.id);
        sqlx::query(
            "UPDATE users SET deleted_at = ?, username = ?, status = 'disabled', updated_at = ? WHERE id = ?",
        )
        .bind(now_ms)
        .bind(anonymized)
        .bind(now_ms)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|e| e.storage_err())?;
        Ok(())
    }
}
