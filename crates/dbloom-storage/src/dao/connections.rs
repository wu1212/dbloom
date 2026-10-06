//! connections 表 DAO（`docs/design/01-data-model.md` §2 连接）。
//!
//! 租户规则（§3.4 + 02-api.md §1）：
//! - 列表查询强制 `owner_user_id` 注入（`TenantScope`：管理员全量 / 显式指定 / 本人）；
//! - 单条读取由上层 handler 用 `TenantScope::can_access_owner` 做 404 判定（防存在性泄漏）；
//! - 软删（`deleted_at`），删除一行不带物理删。

use dbloom_common::AppError;
use sqlx::{FromRow, MySqlPool};

use crate::error::StorageErrorExt;
use crate::tenant::TenantScope;

use super::normalize_page;

/// connections 表行（完整，含密文）。
#[derive(Debug, Clone, FromRow)]
pub struct ConnectionRow {
    pub id: i64,
    pub owner_user_id: i64,
    pub name: String,
    pub conn_type: String,
    pub host: String,
    pub port: Option<i32>,
    pub database_name: Option<String>,
    pub username: Option<String>,
    /// AES-256-GCM 密文（v1.<salt>.<iv>.<ct>）。
    pub password_enc: Option<String>,
    pub ssl_mode: String,
    /// extra_params 的 JSON 文本（MySQL JSON 列读取为字符串）。
    pub extra_params: Option<String>,
    pub is_production: bool,
    pub read_only_lock: bool,
    pub created_at: i64,
    pub updated_at: i64,
    pub deleted_at: Option<i64>,
}

/// 更新字段集合（None=保持原值；password_enc 由上层决定是否写入）。
#[derive(Debug, Clone, Default)]
pub struct ConnectionUpdate {
    pub name: Option<String>,
    pub host: Option<String>,
    pub port: Option<i32>,
    pub database_name: Option<String>,
    pub username: Option<String>,
    pub password_enc: Option<String>, // Some(Some)=新密文；Some(None) 不改变；用 Option<Option<String>> 表达更精确
    pub ssl_mode: Option<String>,
    pub extra_params: Option<Option<String>>,
    pub is_production: Option<bool>,
    pub read_only_lock: Option<bool>,
}

#[derive(Clone)]
pub struct ConnectionDao {
    pool: MySqlPool,
}

impl ConnectionDao {
    pub fn new(pool: MySqlPool) -> Self {
        Self { pool }
    }

    pub async fn create(
        &self,
        owner_user_id: i64,
        name: &str,
        conn_type: &str,
        host: &str,
        port: Option<i32>,
        database_name: Option<&str>,
        username: Option<&str>,
        password_enc: Option<&str>,
        ssl_mode: &str,
        extra_params: Option<&str>, // JSON 文本或 NULL
        is_production: bool,
        read_only_lock: bool,
        now_ms: i64,
    ) -> Result<ConnectionRow, AppError> {
        let res = sqlx::query(
            r#"
            INSERT INTO connections
              (owner_user_id, name, conn_type, host, port, database_name, username,
               password_enc, ssl_mode, extra_params, is_production, read_only_lock,
               created_at, updated_at, deleted_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, CAST(? AS JSON), ?, ?, ?, ?, NULL)
            "#,
        )
        .bind(owner_user_id)
        .bind(name)
        .bind(conn_type)
        .bind(host)
        .bind(port)
        .bind(database_name)
        .bind(username)
        .bind(password_enc)
        .bind(ssl_mode)
        .bind(extra_params)
        .bind(is_production)
        .bind(read_only_lock)
        .bind(now_ms)
        .bind(now_ms)
        .execute(&self.pool)
        .await
        .map_err(|e| e.storage_err())?;
        let id = res.last_insert_id() as i64;
        self.get_by_id(id).await
    }

    /// 按 id 查未软删连接（不含租户条件，由上层做归属判定）。
    pub async fn get_by_id(&self, id: i64) -> Result<ConnectionRow, AppError> {
        sqlx::query_as::<_, ConnectionRow>(
            "SELECT * FROM connections WHERE id = ? AND deleted_at IS NULL LIMIT 1",
        )
        .bind(id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| e.storage_err())
    }

    /// 列表（租户过滤注入；keyword 匹配 name/host；admin 可 ?user_id= 显式跨用户）。
    pub async fn list(
        &self,
        scope: &TenantScope,
        keyword: Option<&str>,
        conn_type: Option<&str>,
        page: Option<i64>,
        page_size: Option<i64>,
    ) -> Result<(i64, Vec<ConnectionRow>), AppError> {
        let (page, size) = normalize_page(page, page_size);
        let offset = (page - 1) * size;

        let kw = keyword.filter(|k| !k.is_empty()).map(|k| format!("%{k}%"));
        let ct = conn_type.filter(|s| !s.is_empty()).map(|s| s.to_string());

        let mut where_sql = String::from("deleted_at IS NULL");
        if !scope.can_see_all() {
            let owner = scope.explicit_user_id.unwrap_or(scope.actor_user_id);
            where_sql.push_str(" AND owner_user_id = ?");
            let _ = owner; // 绑定顺序在下方统一处理
        }
        if kw.is_some() {
            where_sql.push_str(" AND (name LIKE ? OR host LIKE ?)");
        }
        if ct.is_some() {
            where_sql.push_str(" AND conn_type = ?");
        }

        // 收集绑定值（顺序：owner → kw×2 → ct）
        let owner_filter: Option<i64> = if scope.can_see_all() {
            None
        } else {
            Some(scope.explicit_user_id.unwrap_or(scope.actor_user_id))
        };

        let count_sql = format!("SELECT COUNT(*) FROM connections WHERE {where_sql}");
        let total: i64 = {
            let mut q = sqlx::query_scalar::<_, i64>(&count_sql);
            if let Some(o) = owner_filter {
                q = q.bind(o);
            }
            if let Some(k) = kw.as_ref() {
                q = q.bind(k).bind(k);
            }
            if let Some(c) = ct.as_ref() {
                q = q.bind(c);
            }
            q.fetch_one(&self.pool).await.map_err(|e| e.storage_err())?
        };

        let list_sql = format!(
            "SELECT * FROM connections WHERE {where_sql} ORDER BY id DESC LIMIT ? OFFSET ?"
        );
        let mut q = sqlx::query_as::<_, ConnectionRow>(&list_sql);
        if let Some(o) = owner_filter {
            q = q.bind(o);
        }
        if let Some(k) = kw.as_ref() {
            q = q.bind(k).bind(k);
        }
        if let Some(c) = ct.as_ref() {
            q = q.bind(c);
        }
        let items = q
            .bind(size)
            .bind(offset)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| e.storage_err())?;

        Ok((total, items))
    }

    /// 更新：读原行 → 合并变更 → 全列 UPDATE（类型安全；password_enc 传 Some 则覆盖，否则保持）。
    pub async fn update(
        &self,
        id: i64,
        u: &ConnectionUpdate,
        now_ms: i64,
    ) -> Result<ConnectionRow, AppError> {
        let row = self.get_by_id(id).await?;
        let name = u.name.clone().unwrap_or(row.name.clone());
        let host = u.host.clone().unwrap_or(row.host.clone());
        let port = u.port.or(row.port);
        let database_name = u.database_name.clone().or(row.database_name.clone());
        let username = u.username.clone().or(row.username.clone());
        let password_enc = u.password_enc.clone().or(row.password_enc.clone());
        let ssl_mode = u.ssl_mode.clone().unwrap_or(row.ssl_mode.clone());
        let extra_params = u
            .extra_params
            .as_ref()
            .and_then(|v| v.clone())
            .or(row.extra_params.clone());
        let is_production = u.is_production.unwrap_or(row.is_production);
        let read_only_lock = u.read_only_lock.unwrap_or(row.read_only_lock);

        sqlx::query(
            r#"
            UPDATE connections SET
              name=?, host=?, port=?, database_name=?, username=?, password_enc=?,
              ssl_mode=?, extra_params=CAST(? AS JSON), is_production=?, read_only_lock=?,
              updated_at=?
            WHERE id=?
            "#,
        )
        .bind(name)
        .bind(host)
        .bind(port)
        .bind(database_name)
        .bind(username)
        .bind(password_enc)
        .bind(ssl_mode)
        .bind(extra_params)
        .bind(is_production)
        .bind(read_only_lock)
        .bind(now_ms)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|e| e.storage_err())?;
        self.get_by_id(id).await
    }

    /// 软删连接（保审计）。
    pub async fn soft_delete(&self, id: i64, now_ms: i64) -> Result<(), AppError> {
        sqlx::query("UPDATE connections SET deleted_at = ?, updated_at = ? WHERE id = ?")
            .bind(now_ms)
            .bind(now_ms)
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| e.storage_err())?;
        Ok(())
    }

    /// 切换生产只读锁（D6）。
    pub async fn set_read_only_lock(
        &self,
        id: i64,
        locked: bool,
        now_ms: i64,
    ) -> Result<(), AppError> {
        sqlx::query("UPDATE connections SET read_only_lock = ?, updated_at = ? WHERE id = ?")
            .bind(locked)
            .bind(now_ms)
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| e.storage_err())?;
        Ok(())
    }

    /// 统计某用户名下连接数（admin 用户详情用量）。
    pub async fn count_by_owner(&self, owner: i64) -> Result<i64, AppError> {
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM connections WHERE owner_user_id = ? AND deleted_at IS NULL",
        )
        .bind(owner)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| e.storage_err())
    }
}
