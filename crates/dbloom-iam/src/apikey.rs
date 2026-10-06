//! API Key（D9）：生成明文密钥、SHA-256 哈希、前缀提取。
//!
//! 明文格式：`dbk_<40 hex>`。库中只存 `key_hash = SHA-256(明文)` 与
//! `prefix`（展示用）。明文仅在签发响应返回一次。

use dbloom_common::{AppError, Result};
use rand::Rng;
use sha2::{Digest, Sha256};

/// 生成 API Key 明文。
pub fn generate_secret() -> String {
    let mut rng = rand::thread_rng();
    let bytes: Vec<u8> = (0..20).map(|_| rng.gen::<u8>()).collect();
    format!("dbk_{}", hex::encode(bytes))
}

/// SHA-256 哈希（十六进制）。用于库内 `key_hash` 列。
pub fn hash_secret(secret: &str) -> String {
    let mut h = Sha256::new();
    h.update(secret.as_bytes());
    hex::encode(h.finalize())
}

/// 展示前缀（`dbk_<前6位hex>`）。
pub fn prefix_of(secret: &str) -> String {
    let trimmed = secret.trim();
    let keep: String = trimmed.chars().take(10).collect();
    if keep.len() == 10 {
        keep
    } else {
        format!("{keep}…")
    }
}

/// 从 `Authorization: Bearer <x>` 提取 token。
pub fn parse_bearer(header: &str) -> Result<&str> {
    let v = header.trim();
    let rest = v
        .strip_prefix("Bearer ")
        .or_else(|| v.strip_prefix("bearer "))
        .ok_or_else(|| AppError::unauthorized("需要 Bearer 凭据"))?;
    let rest = rest.trim();
    if rest.is_empty() {
        return Err(AppError::unauthorized("Bearer 为空"));
    }
    Ok(rest)
}

/// API Key 生命周期校验（D9：status + valid_from/valid_until，缺省放行）。
/// 返回 None=可用，Some(AppError)=被拒原因（供 authenticate_api_key 与单测共用）。
pub fn validate_key_lifecycle(
    status_enabled: bool,
    valid_from: Option<i64>,
    valid_until: Option<i64>,
    now_ms: i64,
) -> Option<AppError> {
    if !status_enabled {
        return Some(AppError::forbidden("API Key 已停用"));
    }
    if let Some(vf) = valid_from {
        if now_ms < vf {
            return Some(AppError::forbidden("API Key 尚未生效"));
        }
    }
    if let Some(vu) = valid_until {
        if now_ms > vu {
            return Some(AppError::forbidden("API Key 已过期"));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_format_and_hash() {
        let s = generate_secret();
        assert!(s.starts_with("dbk_"));
        assert_eq!(s.len(), 4 + 40);
        assert_eq!(hash_secret(&s), hash_secret(&s));
        assert_ne!(hash_secret(&s), hash_secret("other"));
    }

    #[test]
    fn prefix_is_stable() {
        let s = generate_secret();
        assert_eq!(prefix_of(&s), prefix_of(&s));
    }

    #[test]
    fn parse_bearer_ok() {
        assert_eq!(parse_bearer("Bearer abc").unwrap(), "abc");
        assert!(parse_bearer("Basic abc").is_err());
        assert!(parse_bearer("Bearer ").is_err());
    }

    #[test]
    fn lifecycle_default_ok() {
        // 无生效/失效时间 + enabled → 可用
        assert!(validate_key_lifecycle(true, None, None, 1_000).is_none());
    }

    #[test]
    fn lifecycle_disabled_rejected() {
        assert!(validate_key_lifecycle(false, None, None, 1_000).is_some());
    }

    #[test]
    fn lifecycle_not_yet_valid_rejected() {
        let now = 1_000_000;
        // valid_from 在未来 → 拒
        let e = validate_key_lifecycle(true, Some(now + 1), None, now).unwrap();
        assert!(e.to_string().contains("尚未生效"));
    }

    #[test]
    fn lifecycle_expired_rejected() {
        let now = 1_000_000;
        // valid_until 在过去 → 拒
        let e = validate_key_lifecycle(true, None, Some(now - 1), now).unwrap();
        assert!(e.to_string().contains("已过期"));
    }

    #[test]
    fn lifecycle_boundary_ok() {
        let now = 1_000_000;
        // 恰好等于生效/失效边界 → 可用
        assert!(validate_key_lifecycle(true, Some(now), Some(now), now).is_none());
    }
}
