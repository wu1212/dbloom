//! Iam 聚合服务：持有 4 个 DAO + 配置，暴露高层 API（供 dbloom-server 调用）。
//!
//! 认证中间件（server 侧）通过 `authenticate` 从 Bearer token 解析行为主体；
//! D9：同一套路由共用，JWT（人机）与 API Key（机器）两种来源等价进入租户过滤。

use dbloom_common::{ErrorCode, Result, AppError, time::now_ms};
use dbloom_storage::dao::{ApiKeyDao, AuditDao, SessionDao, UserDao};
use dbloom_types::{
    apikey::{
        ApiKeyDto, ApiKeyStatus, CreateApiKeyRequest, CreateApiKeyResponse, UpdateApiKeyRequest,
    },
    user::{UserDto, UserListResponse, UserRole, UserStatus},
};

use crate::{
    apikey::{self, hash_secret, parse_bearer},
    audit,
    jwt,
    password,
    session,
    users::{self, user_row_to_dto, validate_username},
};

/// Iam 配置。
pub struct Config {
    pub jwt_secret: String,
}

/// 认证结果（中间件注入请求上下文用）。
#[derive(Debug, Clone)]
pub struct AuthResult {
    pub user_id: i64,
    pub username: String,
    pub role: UserRole,
    /// true=本次凭据是 API Key（审计 actor_type='api_key'）。
    pub via_api_key: bool,
    pub api_key_id: Option<i64>,
}

pub struct Iam {
    pub users: UserDao,
    pub sessions: SessionDao,
    pub api_keys: ApiKeyDao,
    pub audit: AuditDao,
    pub config: Config,
}

/// 内置管理员用户名（种子用）。
pub const BUILTIN_ADMIN: &str = "admin";

impl Iam {
    pub fn new(
        users: UserDao,
        sessions: SessionDao,
        api_keys: ApiKeyDao,
        audit: AuditDao,
        config: Config,
    ) -> Self {
        Self {
            users,
            sessions,
            api_keys,
            audit,
            config,
        }
    }

    // ---------------- 认证 ----------------

    pub async fn login(&self, username: &str, password: &str, ip: Option<&str>) -> Result<dbloom_types::auth::LoginResponse> {
        let resp = session::login(&self.users, &self.sessions, &self.config, username, password).await?;
        audit::record_audit(
            &self.audit,
            Some(resp.user.id),
            "user",
            "login",
            Some("user"),
            Some(&resp.user.id.to_string()),
            Some(audit::detail_json(&[("ip", ip.unwrap_or("").to_string())])),
            ip,
        )
        .await?;
        Ok(resp)
    }

    pub async fn refresh(&self, refresh_token: &str) -> Result<dbloom_types::auth::RefreshResponse> {
        session::refresh(&self.users, &self.sessions, &self.config, refresh_token).await
    }

    pub async fn logout(&self, user_id: i64, refresh_token: &str) -> Result<()> {
        session::logout(&self.sessions, user_id, refresh_token).await
    }

    pub async fn change_password(&self, user_id: i64, old: &str, new: &str) -> Result<()> {
        session::change_password(&self.users, &self.sessions, user_id, old, new).await
    }

    /// 从 Bearer token 解析行为主体：JWT（人机）或 API Key（机器，D9）。
    ///
    /// 返回 `AuthResult`。鉴权失败返回对应错误码（40100/40101/403xx）。
    pub async fn authenticate(&self, bearer: &str) -> Result<AuthResult> {
        let token = parse_bearer(bearer)?;
        if token.starts_with("dbk_") {
            self.authenticate_api_key(token).await
        } else {
            self.authenticate_jwt(token).await
        }
    }

    async fn authenticate_jwt(&self, token: &str) -> Result<AuthResult> {
        let claims = jwt::verify_access(&self.config.jwt_secret, token)?;
        let user = self.users.get_by_id(claims.sub).await.map_err(|_| {
            if claims.sub == 0 {
                AppError::unauthorized("用户不存在")
            } else {
                AppError::unauthorized("用户不存在")
            }
        })?;
        self.ensure_user_loginable(&user)?;
        Ok(AuthResult {
            user_id: user.id,
            username: user.username.clone(),
            role: UserRole::parse(&user.role).unwrap_or(UserRole::User),
            via_api_key: false,
            api_key_id: None,
        })
    }

    async fn authenticate_api_key(&self, token: &str) -> Result<AuthResult> {
        let now_ms = now_ms();
        let row = self
            .api_keys
            .get_by_hash(&hash_secret(token))
            .await
            .map_err(|e| {
                if e.code == ErrorCode::NotFound {
                    AppError::unauthorized("无效的 API Key")
                } else {
                    e
                }
            })?;

        if ApiKeyStatus::parse(&row.status) != Some(ApiKeyStatus::Enabled) {
            return Err(AppError::forbidden("API Key 已停用"));
        }
        if let Some(vf) = row.valid_from {
            if now_ms < vf {
                return Err(AppError::forbidden("API Key 尚未生效"));
            }
        }
        if let Some(vu) = row.valid_until {
            if now_ms > vu {
                return Err(AppError::forbidden("API Key 已过期"));
            }
        }

        let user = self.users.get_by_id(row.user_id).await?;
        self.ensure_user_loginable(&user)?;

        self.api_keys.touch_last_used(row.id, now_ms).await.ok();
        Ok(AuthResult {
            user_id: user.id,
            username: user.username.clone(),
            role: UserRole::parse(&user.role).unwrap_or(UserRole::User),
            via_api_key: true,
            api_key_id: Some(row.id),
        })
    }

    fn ensure_user_loginable(&self, user: &dbloom_storage::dao::UserRow) -> Result<()> {
        let status = UserStatus::parse(&user.status).unwrap_or(UserStatus::Active);
        if status == UserStatus::Disabled {
            return Err(AppError::forbidden("账号已被禁用"));
        }
        if let Some(until) = user.locked_until {
            if until > now_ms() {
                return Err(AppError::account_locked("账号已锁定，请稍后再试"));
            }
        }
        Ok(())
    }

    // ---------------- 用户管理（admin 域，D8） ----------------

    pub async fn create_user(
        &self,
        admin_id: i64,
        username: &str,
        display_name: Option<&str>,
    ) -> Result<(UserDto, String)> {
        let _ = display_name; // M0：display_name 预留，创建暂不写入
        validate_username(username)?;
        if self.users.get_by_username(username).await.is_ok() {
            return Err(AppError::conflict("用户名已存在"));
        }
        let initial_password = password::generate_random_password(12);
        let hash = password::hash_password(&initial_password)?;
        let row = self
            .users
            .create(username, &hash, "user", Some(admin_id), now_ms())
            .await?;
        audit::record_audit(
            &self.audit,
            Some(admin_id),
            "user",
            "user_create",
            Some("user"),
            Some(&row.id.to_string()),
            Some(audit::detail_json(&[("target", username.to_string())])),
            None,
        )
        .await?;
        Ok((user_row_to_dto(&row), initial_password))
    }

    pub async fn list_users(
        &self,
        keyword: Option<&str>,
        status: Option<&str>,
        page: Option<i64>,
        page_size: Option<i64>,
    ) -> Result<UserListResponse> {
        let (total, rows) = self.users.list(keyword, status, page, page_size).await?;
        Ok(users::rows_to_list(total, rows))
    }

    pub async fn get_user(&self, id: i64) -> Result<UserDto> {
        Ok(user_row_to_dto(&self.users.get_by_id(id).await?))
    }

    pub async fn update_user(
        &self,
        admin_id: i64,
        id: i64,
        display_name: Option<&str>,
        status: Option<&str>,
    ) -> Result<UserDto> {
        // 防御：禁止禁用/删除内置 admin（避免后台锁死，D20 保障连续性）。
        if let Some(st) = status {
            if st != "active" {
                if let Ok(u) = self.users.get_by_id(id).await {
                    if u.username == BUILTIN_ADMIN {
                        return Err(AppError::conflict("不允许禁用内置管理员账号"));
                    }
                }
            }
        }
        self.users.update_profile(id, display_name, status, now_ms()).await?;
        audit::record_audit(
            &self.audit,
            Some(admin_id),
            "user",
            "user_update",
            Some("user"),
            Some(&id.to_string()),
            None,
            None,
        )
        .await?;
        self.get_user(id).await
    }

    /// 强制重置密码（D20）：新随机密码 + 强制改密 + 吊销会话。
    pub async fn reset_password(&self, admin_id: i64, id: i64) -> Result<String> {
        let user = self.users.get_by_id(id).await?;
        if user.username == BUILTIN_ADMIN {
            return Err(AppError::conflict("内置管理员请使用个人改密或部署重置流程"));
        }
        let new_password = password::generate_random_password(12);
        let hash = password::hash_password(&new_password)?;
        self.users
            .set_password(id, &hash, true, now_ms())
            .await?;
        self.sessions.revoke_all_for_user(id, "password_reset").await?;
        audit::record_audit(
            &self.audit,
            Some(admin_id),
            "user",
            "user_reset_password",
            Some("user"),
            Some(&id.to_string()),
            None,
            None,
        )
        .await?;
        Ok(new_password)
    }

    /// 软删用户（01-data-model §5：匿名化 + 数据保留）。
    pub async fn delete_user(&self, admin_id: i64, id: i64) -> Result<()> {
        let user = self.users.get_by_id(id).await?;
        if user.username == BUILTIN_ADMIN {
            return Err(AppError::conflict("不允许删除内置管理员账号"));
        }
        self.users.soft_delete(id, now_ms()).await?;
        // 相关会话立即失效
        self.sessions.revoke_all_for_user(id, "user_deleted").await?;
        audit::record_audit(
            &self.audit,
            Some(admin_id),
            "user",
            "user_delete",
            Some("user"),
            Some(&id.to_string()),
            None,
            None,
        )
        .await?;
        Ok(())
    }

    // ---------------- 通用审计（D21） ----------------

    /// 记录一条审计（供各业务 handler 复用；actor_type: 'user' | 'api_key'）。
    pub async fn record_audit(
        &self,
        actor_user_id: Option<i64>,
        actor_type: &str,
        action: &str,
        resource_type: Option<&str>,
        resource_id: Option<&str>,
        detail: Option<serde_json::Value>,
        ip: Option<&str>,
    ) -> Result<()> {
        audit::record_audit(
            &self.audit,
            actor_user_id,
            actor_type,
            action,
            resource_type,
            resource_id,
            detail,
            ip,
        )
        .await
    }

    // ---------------- API Key（admin 域，D9） ----------------

    pub async fn issue_api_key(
        &self,
        admin_id: i64,
        target_user_id: i64,
        req: CreateApiKeyRequest,
        ip: Option<&str>,
    ) -> Result<CreateApiKeyResponse> {
        if req.name.trim().is_empty() {
            return Err(AppError::validation("name 不能为空"));
        }
        if let (Some(f), Some(u)) = (req.valid_from, req.valid_until) {
            if f > u {
                return Err(AppError::validation("valid_from 不能晚于 valid_until"));
            }
        }
        let user = self.users.get_by_id(target_user_id).await?;
        let secret = apikey::generate_secret();
        let hash = hash_secret(&secret);
        let prefix = apikey::prefix_of(&secret);
        let row = self
            .api_keys
            .create(
                user.id,
                admin_id,
                &req.name,
                &hash,
                &prefix,
                req.valid_from,
                req.valid_until,
                now_ms(),
            )
            .await?;
        audit::record_audit(
            &self.audit,
            Some(admin_id),
            "user",
            "apikey_issue",
            Some("api_key"),
            Some(&row.id.to_string()),
            Some(audit::detail_json(&[("target_user", user.username.clone())])),
            ip,
        )
        .await?;
        Ok(CreateApiKeyResponse {
            key: self.api_key_to_dto(&row).await?,
            secret,
        })
    }

    pub async fn list_api_keys(&self, user_id: i64) -> Result<Vec<ApiKeyDto>> {
        let rows = self.api_keys.list_by_user(user_id).await?;
        let mut out = Vec::with_capacity(rows.len());
        for r in rows {
            out.push(self.api_key_to_dto(&r).await?);
        }
        Ok(out)
    }

    pub async fn update_api_key(
        &self,
        admin_id: i64,
        key_id: i64,
        req: UpdateApiKeyRequest,
    ) -> Result<ApiKeyDto> {
        self.api_keys
            .update_fields(
                key_id,
                req.name.as_deref(),
                req.status.map(|s| s.as_str()),
                Some(req.valid_from),
                Some(req.valid_until),
                now_ms(),
            )
            .await?;
        audit::record_audit(
            &self.audit,
            Some(admin_id),
            "user",
            "apikey_update",
            Some("api_key"),
            Some(&key_id.to_string()),
            None,
            None,
        )
        .await?;
        let row = self.api_keys.get_by_id(key_id).await?;
        Ok(self.api_key_to_dto(&row).await?)
    }

    /// 撤销 Key（软删，立即全局生效，D9）。
    pub async fn revoke_api_key(&self, admin_id: i64, key_id: i64) -> Result<()> {
        self.api_keys.soft_delete(key_id, now_ms()).await?;
        audit::record_audit(
            &self.audit,
            Some(admin_id),
            "user",
            "apikey_revoke",
            Some("api_key"),
            Some(&key_id.to_string()),
            None,
            None,
        )
        .await?;
        Ok(())
    }

    async fn api_key_to_dto(&self, row: &dbloom_storage::dao::ApiKeyRow) -> Result<ApiKeyDto> {
        let uname = self
            .users
            .get_by_id(row.user_id)
            .await
            .map(|u| u.username)
            .unwrap_or_else(|_| "?".to_string());
        Ok(ApiKeyDto {
            id: row.id,
            user_id: row.user_id,
            user_username: uname,
            name: row.name.clone(),
            prefix: row.prefix.clone(),
            status: ApiKeyStatus::parse(&row.status).unwrap_or(ApiKeyStatus::Enabled),
            valid_from: row.valid_from,
            valid_until: row.valid_until,
            last_used_at: row.last_used_at,
            created_at: row.created_at,
        })
    }
}
