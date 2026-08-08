//! 连接密码本机加密（AES-GCM + 本地密钥文件）

use std::fs;
use std::path::Path;

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use rand::RngCore;
use thiserror::Error;

/// 密文前缀：版本化，便于日后轮换算法
pub const CIPHER_PREFIX: &str = "enc:v1:";

const KEY_LEN: usize = 32;
const NONCE_LEN: usize = 12;

#[derive(Debug, Error)]
pub enum CryptoError {
    #[error("读写密钥失败: {0}")]
    Io(#[from] std::io::Error),
    #[error("密钥长度无效")]
    InvalidKey,
    #[error("密文格式无效")]
    InvalidCiphertext,
    #[error("加密失败")]
    Encrypt,
    #[error("解密失败")]
    Decrypt,
}

/// 基于本地密钥文件的密码加解密
pub struct PasswordCrypto {
    cipher: Aes256Gcm,
}

impl PasswordCrypto {
    /// 加载或创建密钥文件（32 字节原始密钥）
    pub fn load_or_create(key_path: &Path) -> Result<Self, CryptoError> {
        let key_bytes = if key_path.exists() {
            let data = fs::read(key_path)?;
            if data.len() != KEY_LEN {
                return Err(CryptoError::InvalidKey);
            }
            data
        } else {
            if let Some(parent) = key_path.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut key = vec![0u8; KEY_LEN];
            rand::thread_rng().fill_bytes(&mut key);
            fs::write(key_path, &key)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = fs::set_permissions(key_path, fs::Permissions::from_mode(0o600));
            }
            key
        };

        let cipher = Aes256Gcm::new_from_slice(&key_bytes).map_err(|_| CryptoError::InvalidKey)?;
        Ok(Self { cipher })
    }

    /// 是否已是 `enc:v1:` 密文
    pub fn is_encrypted(value: &str) -> bool {
        value.starts_with(CIPHER_PREFIX)
    }

    /// 加密明文，返回 `enc:v1:<base64(nonce||ciphertext)>`
    pub fn encrypt(&self, plaintext: &str) -> Result<String, CryptoError> {
        let mut nonce_bytes = [0u8; NONCE_LEN];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = self
            .cipher
            .encrypt(nonce, plaintext.as_bytes())
            .map_err(|_| CryptoError::Encrypt)?;

        let mut packed = Vec::with_capacity(NONCE_LEN + ciphertext.len());
        packed.extend_from_slice(&nonce_bytes);
        packed.extend_from_slice(&ciphertext);
        Ok(format!("{CIPHER_PREFIX}{}", B64.encode(packed)))
    }

    /// 解密 `enc:v1:` 密文；已是明文或空串时按原样返回（空串不解密）
    pub fn decrypt(&self, token: &str) -> Result<String, CryptoError> {
        if !Self::is_encrypted(token) {
            return Err(CryptoError::InvalidCiphertext);
        }
        let raw = B64
            .decode(&token[CIPHER_PREFIX.len()..])
            .map_err(|_| CryptoError::InvalidCiphertext)?;
        if raw.len() <= NONCE_LEN {
            return Err(CryptoError::InvalidCiphertext);
        }
        let (nonce_bytes, ciphertext) = raw.split_at(NONCE_LEN);
        let nonce = Nonce::from_slice(nonce_bytes);
        let plain = self
            .cipher
            .decrypt(nonce, ciphertext)
            .map_err(|_| CryptoError::Decrypt)?;
        String::from_utf8(plain).map_err(|_| CryptoError::Decrypt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let crypto = PasswordCrypto::load_or_create(&dir.path().join("key")).unwrap();
        let t = crypto.encrypt("s3cret").unwrap();
        assert!(t.starts_with("enc:v1:"));
        assert_eq!(crypto.decrypt(&t).unwrap(), "s3cret");
    }

    #[test]
    fn load_same_key_decrypts() {
        let dir = tempfile::tempdir().unwrap();
        let key = dir.path().join("key");
        let a = PasswordCrypto::load_or_create(&key).unwrap();
        let token = a.encrypt("pw").unwrap();
        let b = PasswordCrypto::load_or_create(&key).unwrap();
        assert_eq!(b.decrypt(&token).unwrap(), "pw");
    }
}
