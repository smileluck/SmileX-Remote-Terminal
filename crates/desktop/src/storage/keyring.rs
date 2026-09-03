//! # 本地加密凭据库（secret vault）
//!
//! 敏感凭据（SSH 密码 / 私钥 / LLM API Key）AES-256-GCM 加密后存于 app 数据目录：
//! - `secret_vault.key`：32 字节主密钥（首次访问时生成，0600 权限）
//! - `secret_vault.json`：`{ key: { nonce, ct } }`（base64）
//!
//! ## 为什么不用 OS Keychain
//! dev 构建为 ad-hoc 签名，每次重编译 code identity 都变化，macOS Keychain
//! 无法识别为同一 app：每次读取凭据都弹系统授权框要求输入登录密码
//! （"始终允许"也随签名变化失效）。正式分发（Developer ID 签名）后可再评估切回。
//! 读取时若 vault 未命中，回退读一次旧 Keychain 数据并自动迁移
//! （老数据迁移会弹最后一次授权，此后不再访问 Keychain）。
//!
//! 安全性说明：密钥与密文同目录，防护级别为"文件权限隔离同机其他用户"，
//! 弱于 Keychain 的按 app 授权模型；对个人终端工具是可接受的折衷。

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use anyhow::{anyhow, Context, Result};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use rand::RngCore;
use serde::{Deserialize, Serialize};

/// 旧 Keychain 服务名（仅用于读取历史数据迁移）
const SERVICE_NAME: &str = "smilex-remote-terminal";
/// Tauri identifier（与 app_data_dir 定位的目录保持一致）
const APP_IDENTIFIER: &str = "com.smilex.remote-terminal";
const VAULT_FILE: &str = "secret_vault.json";
const KEY_FILE: &str = "secret_vault.key";
/// AES-GCM nonce 长度（字节）
const NONCE_LEN: usize = 12;

/// 全局串行锁（vault 文件全量读写，条目量小，简单互斥足够）
static LOCK: Mutex<()> = Mutex::new(());

/// vault 目录（与 SQLite 数据库同目录：`<data_dir>/<identifier>/`）
fn vault_dir() -> Result<PathBuf> {
    let dir = dirs::data_dir()
        .context("无法定位用户数据目录")?
        .join(APP_IDENTIFIER);
    fs::create_dir_all(&dir).with_context(|| format!("创建凭据目录失败: {}", dir.display()))?;
    Ok(dir)
}

#[derive(Serialize, Deserialize, Default)]
struct Vault {
    entries: HashMap<String, VaultEntry>,
}

#[derive(Serialize, Deserialize)]
struct VaultEntry {
    /// base64(12 字节 nonce)
    nonce: String,
    /// base64(密文)
    ct: String,
}

/// 读取（或首次生成）32 字节主密钥
fn load_or_create_key(dir: &Path) -> Result<[u8; 32]> {
    let key_path = dir.join(KEY_FILE);
    if key_path.exists() {
        let raw = fs::read(&key_path).with_context(|| format!("读取密钥文件失败: {}", key_path.display()))?;
        return raw
            .try_into()
            .map_err(|_| anyhow!("密钥文件损坏（长度不符）：{}", key_path.display()));
    }
    let mut key = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut key);
    fs::write(&key_path, key).with_context(|| format!("写入密钥文件失败: {}", key_path.display()))?;
    set_private(&key_path);
    Ok(key)
}

fn load_vault(dir: &Path) -> Result<Vault> {
    let path = dir.join(VAULT_FILE);
    if !path.exists() {
        return Ok(Vault::default());
    }
    let raw = fs::read_to_string(&path).with_context(|| format!("读取凭据库失败: {}", path.display()))?;
    serde_json::from_str(&raw).with_context(|| format!("凭据库文件损坏: {}", path.display()))
}

fn save_vault(dir: &Path, vault: &Vault) -> Result<()> {
    let path = dir.join(VAULT_FILE);
    let json = serde_json::to_string(vault).context("序列化凭据库失败")?;
    fs::write(&path, json).with_context(|| format!("写入凭据库失败: {}", path.display()))?;
    set_private(&path);
    Ok(())
}

/// 设置文件属主读写权限（unix；Windows 忽略，依赖用户目录 ACL）
fn set_private(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
}

fn encrypt(master: &[u8; 32], plaintext: &str) -> Result<VaultEntry> {
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(master));
    let mut nonce_bytes = [0u8; NONCE_LEN];
    rand::rngs::OsRng.fill_bytes(&mut nonce_bytes);
    let ct = cipher
        .encrypt(Nonce::from_slice(&nonce_bytes), plaintext.as_bytes())
        .map_err(|_| anyhow!("凭据加密失败"))?;
    Ok(VaultEntry {
        nonce: B64.encode(nonce_bytes),
        ct: B64.encode(ct),
    })
}

fn decrypt(master: &[u8; 32], entry: &VaultEntry) -> Result<String> {
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(master));
    let nonce_bytes = B64.decode(&entry.nonce).context("nonce 解码失败")?;
    let ct = B64.decode(&entry.ct).context("密文解码失败")?;
    let pt = cipher
        .decrypt(Nonce::from_slice(&nonce_bytes), ct.as_ref())
        .map_err(|_| anyhow!("凭据解密失败（密钥不匹配或数据损坏）"))?;
    String::from_utf8(pt).context("凭据内容非 UTF-8")
}

/// 存储凭据（写入本地加密 vault）
pub fn set_credential(key: &str, value: &str) -> Result<()> {
    let _guard = LOCK.lock().unwrap();
    let dir = vault_dir()?;
    let master = load_or_create_key(&dir)?;
    let mut vault = load_vault(&dir)?;
    vault.entries.insert(key.to_string(), encrypt(&master, value)?);
    save_vault(&dir, &vault)
}

/// 读取凭据：vault 优先；未命中时回退旧 Keychain 并自动迁移
///
/// Keychain 回退仅在存在历史数据时触发（查询不存在的条目不弹授权框）；
/// macOS 上存在历史数据时会弹**最后一次**系统授权。用户拒绝授权时按
/// "无凭据"处理（调用方提示重新输入密码，新密码将直接落 vault）。
pub fn get_credential(key: &str) -> Result<Option<String>> {
    let _guard = LOCK.lock().unwrap();
    let dir = vault_dir()?;
    let mut vault = load_vault(&dir)?;

    if let Some(entry) = vault.entries.get(key) {
        let master = load_or_create_key(&dir)?;
        return decrypt(&master, entry).map(Some);
    }

    // 旧 Keychain 数据迁移（一次性）
    let legacy = keyring::Entry::new(SERVICE_NAME, key)
        .and_then(|e| e.get_password())
        .ok()
        .filter(|s| !s.is_empty());
    if let Some(secret) = legacy {
        let master = load_or_create_key(&dir)?;
        vault.entries.insert(key.to_string(), encrypt(&master, &secret)?);
        save_vault(&dir, &vault)?;
        tracing::info!("已将历史凭据从系统钥匙串迁移至本地加密库: {key}");
        return Ok(Some(secret));
    }
    Ok(None)
}

/// 删除凭据（仅 vault；旧 Keychain 条目不主动清理，避免删除动作再触发授权弹窗）
pub fn delete_credential(key: &str) -> Result<()> {
    let _guard = LOCK.lock().unwrap();
    let dir = vault_dir()?;
    let mut vault = load_vault(&dir)?;
    if vault.entries.remove(key).is_some() {
        save_vault(&dir, &vault)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_vault_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("smilex-vault-test-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn vault_roundtrip_and_delete() {
        let dir = temp_vault_dir("roundtrip");
        set_credential_for(&dir, "session:p1", "hunter2");
        set_credential_for(&dir, "ai:api_key", "sk-test-中文密码ｱ");
        assert_eq!(
            get_credential_for(&dir, "session:p1").unwrap(),
            Some("hunter2".to_string())
        );
        assert_eq!(
            get_credential_for(&dir, "ai:api_key").unwrap(),
            Some("sk-test-中文密码ｱ".to_string())
        );
        assert_eq!(get_credential_for(&dir, "missing").unwrap(), None);

        delete_credential_for(&dir, "session:p1");
        assert_eq!(get_credential_for(&dir, "session:p1").unwrap(), None);

        // 密文文件确实落盘且不含明文
        let raw = fs::read_to_string(dir.join(VAULT_FILE)).unwrap();
        assert!(!raw.contains("sk-test"));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn key_file_persists_across_calls() {
        let dir = temp_vault_dir("keyfile");
        set_credential_for(&dir, "k", "v1");
        // 再次写入新条目应复用已有密钥（旧条目仍可解）
        set_credential_for(&dir, "k2", "v2");
        assert_eq!(get_credential_for(&dir, "k").unwrap(), Some("v1".to_string()));
        fs::remove_dir_all(&dir).unwrap();
    }

    /// 面向测试的参数化内部实现（公开 API 走固定目录 + Keychain 迁移）
    fn set_credential_for(dir: &Path, key: &str, value: &str) {
        let master = load_or_create_key(dir).unwrap();
        let mut vault = load_vault(dir).unwrap();
        vault.entries.insert(key.to_string(), encrypt(&master, value).unwrap());
        save_vault(dir, &vault).unwrap();
    }

    fn get_credential_for(dir: &Path, key: &str) -> Result<Option<String>> {
        let vault = load_vault(dir)?;
        match vault.entries.get(key) {
            Some(entry) => decrypt(&load_or_create_key(dir)?, entry).map(Some),
            None => Ok(None),
        }
    }

    fn delete_credential_for(dir: &Path, key: &str) {
        let mut vault = load_vault(dir).unwrap();
        vault.entries.remove(key);
        save_vault(dir, &vault).unwrap();
    }
}
