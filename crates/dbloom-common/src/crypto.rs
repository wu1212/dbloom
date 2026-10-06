//! 连接凭据加密（`docs/design/04-security.md` §3.1）。
//!
//! - 算法：AES-256-GCM（`aes-gcm` crate）。
//! - 密文格式：`v1.<salt_b64>.<iv_b64>.<ciphertext_b64>`。
//! - 密钥派生：`主密钥 = SHA-256(key_material)`；每次加密用随机 salt
//!   派生 `enc_key = SHA-256(主密钥 || salt)`（每份密文独立密钥，支持轮换/刷盐）。
//! - key_material 来源（`DBLOOM_ENV=production` 时至少一项必须配置，否则拒绝启动）：
//!   1. `DBLOOM_SECRET_KEY_FILE`：指向密钥文件，读取其内容作为材料；
//!   2. `DBLOOM_SECRET_KEY`：环境变量直接给材料；
//!   3. （非生产）两者都缺 → 从 JWT secret SHA-256 派生并告警。

use crate::{AppError, Result};
use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use rand::RngCore;
use sha2::{Digest, Sha256};
use std::path::Path;

/// 密文格式版本。
pub const CIPHER_VERSION: &str = "v1";
const SALT_LEN: usize = 16;
const IV_LEN: usize = 12; // AES-GCM 推荐 nonce 96-bit

/// 凭据加密器：持 32 字节主密钥。
#[derive(Debug, Clone)]
pub struct CryptoProvider {
    master: [u8; 32],
}

impl CryptoProvider {
    /// 从环境变量/密钥文件加载主密钥（启动时调用，生产缺失即失败）。
    pub fn from_env(jwt_secret: &str) -> Result<Self> {
        let production = std::env::var("DBLOOM_ENV").map(|e| e.eq_ignore_ascii_case("production"))
            .unwrap_or(false);

        let material: Option<String> = read_secret_file()
            .or_else(|| std::env::var("DBLOOM_SECRET_KEY").ok());

        let key_material = match material {
            Some(m) if !m.trim().is_empty() => m,
            _ if production => {
                return Err(AppError::internal(
                    "生产环境必须配置密钥：DBLOOM_SECRET_KEY_FILE 或 DBLOOM_SECRET_KEY（04-security.md §3.4）",
                ));
            }
            _ => {
                tracing::warn!(
                    "未配置 DBLOOM_SECRET_KEY_FILE/DBLOOM_SECRET_KEY，开发模式从 JWT secret 派生主密钥（生产必须显式配置）"
                );
                jwt_secret.to_string()
            }
        };

        Ok(Self {
            master: derive_key(key_material.as_bytes()),
        })
    }

    /// 加密明文为 `v1.<salt>.<iv>.<ciphertext>`（均为 base64、无填充歧义）。
    pub fn encrypt_str(&self, plain: &str) -> Result<String> {
        let mut salt = [0u8; SALT_LEN];
        rand::thread_rng().fill_bytes(&mut salt);
        let mut iv = [0u8; IV_LEN];
        rand::thread_rng().fill_bytes(&mut iv);

        let enc_key = derive_key_parts(&self.master, &salt);
        let cipher = Aes256Gcm::new_from_slice(&enc_key)
            .map_err(|e| AppError::internal(format!("初始化 AES-GCM 失败: {e}")))?;
        let ct = cipher
            .encrypt(
                Nonce::from_slice(&iv),
                Payload { msg: plain.as_bytes(), aad: CIPHER_VERSION.as_bytes() },
            )
            .map_err(|_| AppError::internal("凭据加密失败"))?;

        Ok(format!(
            "{CIPHER_VERSION}.{}.{}.{}",
            B64.encode(salt),
            B64.encode(iv),
            B64.encode(ct)
        ))
    }

    /// 解密：解析格式 → 派生密钥 → AES-GCM 解密。
    pub fn decrypt_str(&self, ciphertext: &str) -> Result<String> {
        let parts: Vec<&str> = ciphertext.splitn(4, '.').collect();
        if parts.len() != 4 || parts[0] != CIPHER_VERSION {
            return Err(AppError::internal("凭据密文格式非法（期望 v1.<salt>.<iv>.<ct>）"));
        }
        let salt = B64
            .decode(parts[1])
            .map_err(|_| AppError::internal("凭据解码失败: salt"))?;
        let iv = B64
            .decode(parts[2])
            .map_err(|_| AppError::internal("凭据解码失败: iv"))?;
        let ct = B64
            .decode(parts[3])
            .map_err(|_| AppError::internal("凭据解码失败: ciphertext"))?;
        if iv.len() != IV_LEN {
            return Err(AppError::internal("凭据 iv 长度非法"));
        }

        let enc_key = derive_key_parts(&self.master, &salt);
        let cipher = Aes256Gcm::new_from_slice(&enc_key)
            .map_err(|e| AppError::internal(format!("初始化 AES-GCM 失败: {e}")))?;
        let plain = cipher
            .decrypt(
                Nonce::from_slice(&iv),
                Payload { msg: &ct, aad: CIPHER_VERSION.as_bytes() },
            )
            .map_err(|_| AppError::internal("凭据解密失败（主密钥与加密时不一致？）"))?;
        String::from_utf8(plain).map_err(|e| AppError::internal(format!("凭据明文非法: {e}")))
    }
}

/// 读取 `DBLOOM_SECRET_KEY_FILE` 指向的文件内容（去除首尾空白与换行）。
fn read_secret_file() -> Option<String> {
    let path = std::env::var("DBLOOM_SECRET_KEY_FILE").ok()?;
    match std::fs::read_to_string(Path::new(&path)) {
        Ok(content) => Some(content.trim().to_string()),
        Err(e) => {
            tracing::warn!("DBLOOM_SECRET_KEY_FILE 读取失败（{path}）: {e}");
            None
        }
    }
}

/// 固定派生：材料 → 32 字节主密钥。
fn derive_key(material: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(material);
    hasher.finalize().into()
}

/// 加盐派生：主密钥 + 随机 salt → 本次加密密钥。
fn derive_key_parts(master: &[u8; 32], salt: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(master);
    hasher.update(b"||dbloom-sep||");
    hasher.update(salt);
    hasher.finalize().into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_encrypt_decrypt() {
        let mut raw = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut raw);
        let provider = CryptoProvider { master: raw };
        let plain = "S3cr3t-P@ssw0rd-中文-测试";
        let ct = provider.encrypt_str(plain).unwrap();
        assert!(!ct.contains(plain));
        assert!(ct.starts_with("v1."));
        assert_eq!(provider.decrypt_str(&ct).unwrap(), plain);
    }

    #[test]
    fn different_salt_gives_different_ciphertext() {
        let mut raw = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut raw);
        let provider = CryptoProvider { master: raw };
        let a = provider.encrypt_str("same").unwrap();
        let b = provider.encrypt_str("same").unwrap();
        assert_ne!(a, b, "随机盐应使同明文密文不同");
    }

    #[test]
    fn wrong_key_fails() {
        let mut a = [0u8; 32];
        let mut b = [1u8; 32];
        rand::thread_rng().fill_bytes(&mut a);
        rand::thread_rng().fill_bytes(&mut b);
        let p1 = CryptoProvider { master: a };
        let p2 = CryptoProvider { master: b };
        let ct = p1.encrypt_str("data").unwrap();
        assert!(p2.decrypt_str(&ct).is_err());
    }

    #[test]
    fn corrupt_ciphertext_fails() {
        let mut raw = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut raw);
        let provider = CryptoProvider { master: raw };
        let ct = provider.encrypt_str("data").unwrap();
        let mut bytes = ct.into_bytes();
        let last = bytes.len() - 1;
        bytes[last] = if bytes[last] == b'A' { b'B' } else { b'A' };
        assert!(provider.decrypt_str(&String::from_utf8(bytes).unwrap()).is_err());
    }
}
