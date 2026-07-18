//! # OS Keyring 凭据存储
//!
//! 跨平台抽象（Win Credential Store / macOS Keychain / Linux libsecret）。
//! 阶段 2 完整实现，当前为骨架。

use anyhow::Result;
use keyring::Entry;

/// Keyring 服务名（隔离命名空间）
const SERVICE_NAME: &str = "smilex-remote-terminal";

/// 存储凭据
pub fn set_credential(key: &str, value: &str) -> Result<()> {
    let entry = Entry::new(SERVICE_NAME, key)?;
    entry.set_password(value)?;
    Ok(())
}

/// 读取凭据
pub fn get_credential(key: &str) -> Result<Option<String>> {
    let entry = Entry::new(SERVICE_NAME, key)?;
    match entry.get_password() {
        Ok(v) => Ok(Some(v)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// 删除凭据
pub fn delete_credential(key: &str) -> Result<()> {
    let entry = Entry::new(SERVICE_NAME, key)?;
    match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.into()),
    }
}
