//! 密码哈希与随机密码生成（D19/D20）。

use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use dbloom_common::{AppError, Result};
use rand::Rng;

/// argon2 计算密码哈希（返回 PHC 字符串）。
pub fn hash_password(password: &str) -> Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    argon2
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| AppError::internal(format!("密码哈希失败: {e}")))
}

/// 校验密码。
pub fn verify_password(hash: &str, password: &str) -> bool {
    PasswordHash::new(hash)
        .map(|parsed| Argon2::default().verify_password(password.as_bytes(), &parsed).is_ok())
        .unwrap_or(false)
}

const PASSWORD_CHARSET: &[u8] =
    b"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghjkmnpqrstuvwxyz23456789!@#%^&*";

/// 生成随机初始密码（避免形近字符，长度可配）。
pub fn generate_random_password(len: usize) -> String {
    let mut rng = rand::thread_rng();
    (0..len.max(8))
        .map(|_| {
            let idx = rng.gen_range(0..PASSWORD_CHARSET.len());
            PASSWORD_CHARSET[idx] as char
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_verify_roundtrip() {
        let hash = hash_password("secret123").unwrap();
        assert!(verify_password(&hash, "secret123"));
        assert!(!verify_password(&hash, "wrong"));
    }

    #[test]
    fn random_password_conforms() {
        let p = generate_random_password(12);
        assert_eq!(p.len(), 12);
        assert!(p.chars().all(|c| c.is_ascii()));
    }
}
