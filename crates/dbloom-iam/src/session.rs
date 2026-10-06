//! 会话：登录 / 刷新 / 登出 / 改密（D19/D20）。
//! refresh token 只存哈希（`sessions.refresh_hash`）。

use dbloom_common::{time::now_ms, AppError, Result};
use dbloom_storage::dao::{SessionDao, UserDao};
use sha2::{Digest, Sha256};

use crate::jwt::{generate_refresh_token, refresh_expires_at_ms, sign_access};
use crate::password::verify_password;
use crate::users::{user_row_to_dto, validate_new_password};
use crate::Config;
use dbloom_types::{
    auth::{LoginResponse, RefreshResponse},
    user::UserStatus,
};

/// 登录失败锁定阈值与窗口（D19：5 次 / 5 分钟）。
pub const LOGIN_FAILURE_LIMIT: i32 = 5;
pub const LOCK_DURATION_MS: i64 = 5 * 60 * 1000;

fn sha256_hex(s: &str) -> String {
    let mut h = Sha256::new();
    h.update(s.as_bytes());
    hex::encode(h.finalize())
}

fn check_user_status_ok(username: &str, status: &str, locked_until: Option<i64>, now: i64) -> Result<()> {
    let status = UserStatus::parse(status).unwrap_or(UserStatus::Active);
    if status == UserStatus::Disabled {
        return Err(AppError::forbidden("账号已被禁用"));
    }
    if let Some(until) = locked_until {
        if until > now {
            return Err(AppError::account_locked("账号已锁定，请稍后再试"));
        }
    }
    let _ = username; // 一般错误消息不区分用户名是否存在
    Ok(())
}

/// 登录（02-api.md §2.1 POST /auth/login）。
/// 通用错误消息「用户名或密码错误」，避免用户枚举。
pub async fn login(
    users: &UserDao,
    sessions: &SessionDao,
    cfg: &Config,
    username: &str,
    password: &str,
) -> Result<LoginResponse> {
    let now = now_ms();
    let row = users
        .get_by_username(username)
        .await
        .map_err(|_| AppError::unauthorized("用户名或密码错误"))?;

    check_user_status_ok(username, &row.status, row.locked_until, now)?;

    if !verify_password(&row.password_hash, password) {
        let count = users.inc_failed_login(row.id, now).await?;
        if count >= LOGIN_FAILURE_LIMIT {
            users
                .set_locked(row.id, Some(now + LOCK_DURATION_MS), now)
                .await?;
            return Err(AppError::account_locked(
                "连续登录失败次数过多，账号已锁定 5 分钟",
            ));
        }
        return Err(AppError::unauthorized("用户名或密码错误"));
    }

    users.clear_failed_login(row.id, now).await?;
    users.set_last_login(row.id, now).await?;

    let access_token = sign_access(&cfg.jwt_secret, row.id, &row.username, &row.role)?;
    let refresh_token = generate_refresh_token();
    let refresh_hash = sha256_hex(&refresh_token);
    sessions
        .create(row.id, &refresh_hash, refresh_expires_at_ms(), now)
        .await?;

    Ok(LoginResponse {
        access_token,
        refresh_token,
        user: user_row_to_dto(&row),
    })
}

/// 刷新 access token（持有 refresh）。
pub async fn refresh(
    users: &UserDao,
    sessions: &SessionDao,
    cfg: &Config,
    refresh_token: &str,
) -> Result<RefreshResponse> {
    let now = now_ms();
    let refresh_hash = sha256_hex(refresh_token);
    let sess = sessions
        .find_by_hash(&refresh_hash)
        .await
        .map_err(|_| AppError::refresh_invalid("无效的 refresh token"))?;

    if sess.revoke_reason.is_some() {
        return Err(AppError::refresh_invalid("会话已吊销"));
    }
    if sess.expires_at < now {
        return Err(AppError::refresh_invalid("会话已过期"));
    }

    let user = users.get_by_id(sess.user_id).await?;
    check_user_status_ok(&user.username, &user.status, user.locked_until, now)?;

    sessions.update_last_used(sess.id, now).await?;
    let access_token = sign_access(&cfg.jwt_secret, user.id, &user.username, &user.role)?;
    Ok(RefreshResponse { access_token })
}

/// 登出（吊销该 refresh 会话）。
pub async fn logout(
    sessions: &SessionDao,
    user_id: i64,
    refresh_token: &str,
) -> Result<()> {
    let now = now_ms();
    let refresh_hash = sha256_hex(refresh_token);
    let sess = sessions.find_by_hash(&refresh_hash).await?;
    if sess.user_id == user_id {
        sessions.revoke(sess.id, "logout").await?;
    }
    let _ = now;
    Ok(())
}

/// 修改/强制重置自己密码（D20：含首登强改）。成功后吊销该用户全部会话。
pub async fn change_password(
    users: &UserDao,
    sessions: &SessionDao,
    user_id: i64,
    old_password: &str,
    new_password: &str,
) -> Result<()> {
    validate_new_password(new_password)?;
    let user = users.get_by_id(user_id).await?;
    if !verify_password(&user.password_hash, old_password) {
        return Err(AppError::unauthorized("原密码错误"));
    }
    let new_hash = crate::password::hash_password(new_password)?;
    users.set_password(user_id, &new_hash, false, now_ms()).await?;
    sessions
        .revoke_all_for_user(user_id, "password_reset")
        .await?;
    Ok(())
}
