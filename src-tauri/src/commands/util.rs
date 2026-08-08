//! 命令层共用工具

use rand::RngCore;

use crate::config::ConfigStore;
use crate::crypto::PasswordCrypto;
use crate::models::{AppConfig, ConnectionConfig};
use crate::preview_cache::RuleTarget;

/// 生成带前缀的短 id
pub fn new_id(prefix: &str) -> String {
    let mut bytes = [0u8; 8];
    rand::thread_rng().fill_bytes(&mut bytes);
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    format!("{prefix}-{hex}")
}

/// 解密连接密码；空串原样返回
pub fn decrypt_password(store: &ConfigStore, conn: &ConnectionConfig) -> Result<String, String> {
    if conn.password.is_empty() {
        return Ok(String::new());
    }
    if PasswordCrypto::is_encrypted(&conn.password) {
        store
            .crypto()
            .decrypt(&conn.password)
            .map_err(|e| e.to_string())
    } else {
        Ok(conn.password.clone())
    }
}

pub fn find_connection<'a>(
    config: &'a AppConfig,
    id: &str,
) -> Result<&'a ConnectionConfig, String> {
    config
        .connections
        .iter()
        .find(|c| c.id == id)
        .ok_or_else(|| format!("未知连接: {id}"))
}

/// 掩码或空密码视为「未改动」
pub fn is_password_unchanged(password: &str) -> bool {
    password.is_empty() || password == "********"
}

pub fn same_target(a: &RuleTarget, b: &RuleTarget) -> bool {
    a.connection_id == b.connection_id && a.database == b.database
}
