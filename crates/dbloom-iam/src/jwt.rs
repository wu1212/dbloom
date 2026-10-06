//! JWT：access token（2h）签发与校验 + refresh token 生成。
//!
//! Claims 约定（`03-modules.md` §2.4）：`sub` + `role`（+ 可选 `username`）。
//! 签名密钥来自配置 `jwt_secret`（生产必须注入 `DBLOOM_JWT_SECRET`）。

use dbloom_common::{AppError, Result};
use dbloom_common::time::now_ms;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

pub const ACCESS_TTL_SECS: i64 = 2 * 3600; // 2h
pub const REFRESH_TTL_SECS: i64 = 7 * 24 * 3600; // 7d

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// 用户 id
    pub sub: i64,
    pub username: String,
    pub role: String,
    /// 过期时间（Unix 秒）
    pub exp: i64,
}

/// 生成 refresh token（随机 32 字节 hex 明文），调用方只存哈希。
pub fn generate_refresh_token() -> String {
    uuid::Uuid::new_v4().to_string() + &uuid::Uuid::new_v4().to_string().replace('-', "")
}

/// 签发 access JWT。
pub fn sign_access(secret: &str, user_id: i64, username: &str, role: &str) -> Result<String> {
    let exp = now_secs() + ACCESS_TTL_SECS;
    let claims = Claims {
        sub: user_id,
        username: username.to_string(),
        role: role.to_string(),
        exp,
    };
    let header = jsonwebtoken::Header::default();
    jsonwebtoken::encode(
        &header,
        &claims,
        &jsonwebtoken::EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|e| AppError::internal(format!("JWT 签发失败: {e}")))
}

/// 校验 access JWT，返回 claims。过期区分错误码 40101（前端据此刷新，02-api.md §1）。
pub fn verify_access(secret: &str, token: &str) -> std::result::Result<Claims, AppError> {
    let mut validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::HS256);
    validation.validate_exp = true;
    // 允许 ±30s 时钟漂移
    validation.leeway = 30;

    match jsonwebtoken::decode::<Claims>(
        token,
        &jsonwebtoken::DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    ) {
        Ok(data) => Ok(data.claims),
        Err(e) => match e.kind() {
            jsonwebtoken::errors::ErrorKind::ExpiredSignature => Err(AppError::token_expired()),
            _ => Err(AppError::unauthorized("无效的 access token")),
        },
    }
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// refresh token 过期时间（ms）。
pub fn refresh_expires_at_ms() -> i64 {
    now_ms() + REFRESH_TTL_SECS * 1000
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_and_verify_roundtrip() {
        let secret = "test-secret".to_string();
        let token = sign_access(&secret, 1, "alice", "user").unwrap();
        let claims = verify_access(&secret, &token).unwrap();
        assert_eq!(claims.sub, 1);
        assert_eq!(claims.username, "alice");
        assert_eq!(claims.role, "user");
    }

    #[test]
    fn wrong_secret_rejected() {
        let token = sign_access("a", 1, "b", "user").unwrap();
        assert!(verify_access("z", &token).is_err());
    }

    #[test]
    fn refresh_token_is_unique() {
        let a = generate_refresh_token();
        let b = generate_refresh_token();
        assert_ne!(a, b);
        assert!(a.len() > 32);
    }
}
