//! # SSH 密钥管理命令组
//!
//! 生成 / 导入 / 删除 SSH 密钥对：
//! - 元数据（名称/类型/公钥/指纹）→ SQLite `ssh_keys` 表
//! - 私钥 PEM → OS Keyring（key: `ssh_key:{id}:private`），永不落库

use tauri::State;

use crate::error::AppError;
use crate::storage::keyring;
use crate::storage::sqlite::SshKeyMeta;
use crate::AppState;

fn keyring_key(id: &str) -> String {
    format!("ssh_key:{id}:private")
}

/// 全部密钥（元数据）
#[tauri::command]
pub async fn ssh_key_list(state: State<'_, AppState>) -> Result<Vec<SshKeyMeta>, AppError> {
    state
        .storage
        .list_ssh_keys()
        .await
        .map_err(|e| AppError::storage(format!("查询密钥列表失败: {e}")))
}

/// 生成新密钥对（Ed25519 / RSA）
#[tauri::command]
pub async fn ssh_key_generate(
    state: State<'_, AppState>,
    name: String,
    key_type: String,
) -> Result<SshKeyMeta, AppError> {
    let key_type_enum = match key_type.as_str() {
        "rsa" => ssh_core::keys::KeyType::Rsa,
        _ => ssh_core::keys::KeyType::Ed25519,
    };
    let config = ssh_core::keys::KeyGenConfig {
        key_type: key_type_enum,
        comment: Some(name.clone()),
        passphrase: None,
    };
    let generated =
        ssh_core::keys::generate(&config).await.map_err(|e| {
            AppError::session(format!("生成密钥失败: {e}"))
        })?;

    let id = uuid::Uuid::new_v4().to_string();
    // 私钥 → Keyring
    keyring::set_credential(&keyring_key(&id), &generated.private_key_pem)
        .map_err(|e| AppError::storage(format!("私钥写入 Keyring 失败: {e}")))?;

    let meta = SshKeyMeta {
        id,
        name,
        key_type: key_type_enum.as_str().to_string(),
        public_key: generated.public_key,
        fingerprint: generated.fingerprint,
        created_at: chrono::Utc::now().timestamp(),
    };
    state
        .storage
        .save_ssh_key(&meta)
        .await
        .map_err(|e| AppError::storage(format!("保存密钥元数据失败: {e}")))?;
    Ok(meta)
}

/// 导入已有私钥（OpenSSH PEM）
#[tauri::command]
pub async fn ssh_key_import(
    state: State<'_, AppState>,
    name: String,
    private_pem: String,
    passphrase: Option<String>,
) -> Result<SshKeyMeta, AppError> {
    let (fingerprint, public_key) =
        ssh_core::keys::parse_private_key(&private_pem, passphrase.as_deref())
            .map_err(|e| AppError::session(format!("导入失败: {e}")))?;

    let id = uuid::Uuid::new_v4().to_string();
    keyring::set_credential(&keyring_key(&id), &private_pem)
        .map_err(|e| AppError::storage(format!("私钥写入 Keyring 失败: {e}")))?;

    // 公钥行首段为算法名
    let key_type = public_key.split(' ').next().unwrap_or("ssh-ed25519").to_string();
    let meta = SshKeyMeta {
        id,
        name,
        key_type,
        public_key,
        fingerprint,
        created_at: chrono::Utc::now().timestamp(),
    };
    state
        .storage
        .save_ssh_key(&meta)
        .await
        .map_err(|e| AppError::storage(format!("保存密钥元数据失败: {e}")))?;
    Ok(meta)
}

/// 删除密钥（含 Keyring 私钥）
#[tauri::command]
pub async fn ssh_key_delete(state: State<'_, AppState>, id: String) -> Result<bool, AppError> {
    let _ = keyring::delete_credential(&keyring_key(&id));
    state
        .storage
        .delete_ssh_key(&id)
        .await
        .map_err(|e| AppError::storage(format!("删除密钥失败: {e}")))
}

/// 读取私钥（连接时由前端 buildConfig 调用，仅内存使用）
#[tauri::command]
pub async fn ssh_key_get_private(
    state: State<'_, AppState>,
    id: String,
) -> Result<Option<String>, AppError> {
    // 校验密钥确实存在于库中（避免任意读 Keyring）
    let exists = state
        .storage
        .list_ssh_keys()
        .await
        .map_err(|e| AppError::storage(format!("查询密钥失败: {e}")))?
        .iter()
        .any(|k| k.id == id);
    if !exists {
        return Err(AppError::storage(format!("密钥 {id} 不存在")));
    }
    keyring::get_credential(&keyring_key(&id))
        .map_err(|e| AppError::storage(format!("读取私钥失败: {e}")))
}
