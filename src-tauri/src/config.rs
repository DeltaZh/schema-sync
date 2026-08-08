//! 配置文件读写（保存时加密明文密码）

use std::fs;
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::crypto::{CryptoError, PasswordCrypto};
use crate::models::{AppConfig, ConnectionConfig};
use crate::paths::{config_file_path, key_file_path};

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON 错误: {0}")]
    Json(#[from] serde_json::Error),
    #[error("加密错误: {0}")]
    Crypto(#[from] CryptoError),
}

/// 配置仓库：负责 load/save 与密码加密
pub struct ConfigStore {
    config_path: PathBuf,
    crypto: PasswordCrypto,
}

impl ConfigStore {
    /// 在指定数据目录创建/打开配置仓库
    pub fn open(data_dir: &Path) -> Result<Self, ConfigError> {
        fs::create_dir_all(data_dir)?;
        let crypto = PasswordCrypto::load_or_create(&key_file_path(data_dir))?;
        Ok(Self {
            config_path: config_file_path(data_dir),
            crypto,
        })
    }

    pub fn config_path(&self) -> &Path {
        &self.config_path
    }

    pub fn crypto(&self) -> &PasswordCrypto {
        &self.crypto
    }

    /// 加载配置；文件不存在则返回空配置
    pub fn load(&self) -> Result<AppConfig, ConfigError> {
        if !self.config_path.exists() {
            return Ok(AppConfig::default());
        }
        let text = fs::read_to_string(&self.config_path)?;
        if text.trim().is_empty() {
            return Ok(AppConfig::default());
        }
        Ok(serde_json::from_str(&text)?)
    }

    /// 保存配置：明文密码先加密再落盘
    pub fn save(&self, mut config: AppConfig) -> Result<(), ConfigError> {
        for conn in &mut config.connections {
            encrypt_password_in_place(&self.crypto, conn)?;
        }
        if let Some(parent) = self.config_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let text = serde_json::to_string_pretty(&config)?;
        fs::write(&self.config_path, text)?;
        Ok(())
    }

    /// 对外连接列表：密码掩码
    pub fn public_connections(&self, config: &AppConfig) -> Vec<ConnectionConfig> {
        config.connections.iter().map(|c| c.public_view()).collect()
    }
}

fn encrypt_password_in_place(
    crypto: &PasswordCrypto,
    conn: &mut ConnectionConfig,
) -> Result<(), ConfigError> {
    if conn.password.is_empty() {
        return Ok(());
    }
    if PasswordCrypto::is_encrypted(&conn.password) {
        return Ok(());
    }
    conn.password = crypto.encrypt(&conn.password)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ConnectionConfig, NamingRule, PartKind};

    fn sample_conn(password: &str) -> ConnectionConfig {
        ConnectionConfig {
            id: "c1".into(),
            name: "本地".into(),
            host: "127.0.0.1".into(),
            port: 3306,
            user: "root".into(),
            password: password.into(),
            enabled: true,
            remark: String::new(),
        }
    }

    #[test]
    fn save_encrypts_plaintext_password() {
        let dir = tempfile::tempdir().unwrap();
        let store = ConfigStore::open(dir.path()).unwrap();
        let cfg = AppConfig {
            connections: vec![sample_conn("plain-secret")],
            rules: vec![],
        };
        store.save(cfg).unwrap();

        let text = fs::read_to_string(store.config_path()).unwrap();
        assert!(text.contains("enc:v1:"));
        assert!(!text.contains("plain-secret"));

        let loaded = store.load().unwrap();
        assert!(PasswordCrypto::is_encrypted(&loaded.connections[0].password));
        assert_eq!(
            store.crypto().decrypt(&loaded.connections[0].password).unwrap(),
            "plain-secret"
        );
    }

    #[test]
    fn public_connections_masks_password() {
        let dir = tempfile::tempdir().unwrap();
        let store = ConfigStore::open(dir.path()).unwrap();
        let cfg = AppConfig {
            connections: vec![sample_conn("secret")],
            rules: vec![],
        };
        store.save(cfg).unwrap();
        let loaded = store.load().unwrap();
        let public = store.public_connections(&loaded);
        assert_eq!(public[0].password, "********");
        assert_ne!(loaded.connections[0].password, "********");
    }

    #[test]
    fn save_preserves_already_encrypted() {
        let dir = tempfile::tempdir().unwrap();
        let store = ConfigStore::open(dir.path()).unwrap();
        let encrypted = store.crypto().encrypt("once").unwrap();
        store
            .save(AppConfig {
                connections: vec![sample_conn(&encrypted)],
                rules: vec![],
            })
            .unwrap();
        let loaded = store.load().unwrap();
        assert_eq!(loaded.connections[0].password, encrypted);
    }

    #[test]
    fn roundtrip_rules() {
        let dir = tempfile::tempdir().unwrap();
        let store = ConfigStore::open(dir.path()).unwrap();
        let cfg = AppConfig {
            connections: vec![],
            rules: vec![NamingRule {
                id: "r1".into(),
                logical_name: "order".into(),
                parts_order: vec![PartKind::Tenant, PartKind::Year, PartKind::Shard],
                tenants: vec!["lemi".into()],
                years: vec!["2025".into()],
                shards: vec!["1".into()],
                connection_ids: vec!["c1".into()],
            }],
        };
        store.save(cfg.clone()).unwrap();
        assert_eq!(store.load().unwrap(), cfg);
    }
}
