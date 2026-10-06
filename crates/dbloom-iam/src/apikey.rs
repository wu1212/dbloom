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
}
