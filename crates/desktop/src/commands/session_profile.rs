//! # 会话配置命令组
//!
//! 对接前端会话列表管理：保存 / 列出 / 获取 / 删除。
//!
//! ## 敏感字段处理
//! 密码、私钥保护口令**不落 SQLite**，单独存入 OS Keyring（[`crate::storage::keyring`]）。
//! - Keyring key 格式：`session:{profile_id}`
//! - SQLite 只保存非敏感元数据（主机/端口/用户名/认证方式/附加配置）
//!
//! ## extra 字段约定（JSON）
//! ```json
//! {
//!   "private_key_path": "/home/u/.ssh/id_rsa",  // 私钥路径（非敏感）
//!   "accept_first_host_key": false,              // 是否自动接受 host key
//!   "width": 1920, "height": 1080,               // 远程桌面分辨率
//!   "color_depth": 32                             // 远程桌面色深
//! }
//! ```

use tauri::State;

use crate::storage::keyring;
use crate::storage::sqlite::SessionProfile;
use crate::AppState;

/// Keyring key 前缀（与 profile id 拼接）
const KEYRING_PREFIX: &str = "session:";

/// 构造 Keyring key
fn keyring_key(profile_id: &str) -> String {
    format!("{KEYRING_PREFIX}{profile_id}")
}

/// 保存或更新会话配置（含敏感字段同步到 Keyring）
///
/// - `profile`: 非敏感元数据（落 SQLite）
/// - `secret`: 可选敏感字段（密码 / 私钥口令）。None 表示不更新凭据。
///
/// 返回保存后的 profile（含 last_used_at 处理后的最终值）。
#[tauri::command]
pub async fn session_profile_save(
    state: State<'_, AppState>,
    mut profile: SessionProfile,
    secret: Option<String>,
) -> Result<SessionProfile, crate::error::AppError> {
    // 新建时若 created_at 未填，补当前时间
    if profile.created_at == 0 {
        profile.created_at = chrono::Utc::now().timestamp();
    }

    // 持久化到 SQLite（UPSERT）
    state
        .storage
        .save_profile(&profile)
        .await
        .map_err(|e| crate::error::AppError::storage(format!("保存会话配置失败: {e}")))?;

    // 同步敏感字段到 Keyring
    if let Some(secret_value) = secret {
        if !secret_value.is_empty() {
            keyring::set_credential(&keyring_key(&profile.id), &secret_value)
                .map_err(|e| crate::error::AppError::storage(format!("写入 Keyring 失败: {e}")))?;
        } else {
            // 空字符串视为清除凭据
            let _ = keyring::delete_credential(&keyring_key(&profile.id));
        }
    }

    tracing::info!(profile_id = %profile.id, name = %profile.name, "会话配置已保存");
    Ok(profile)
}

/// 列出所有会话配置（按最近使用排序）
#[tauri::command]
pub async fn session_profile_list(
    state: State<'_, AppState>,
) -> Result<Vec<SessionProfile>, crate::error::AppError> {
    state
        .storage
        .list_profiles()
        .await
        .map_err(|e| crate::error::AppError::storage(format!("列出会话配置失败: {e}")))
}

/// 按 id 获取会话配置（不返回敏感字段）
#[tauri::command]
pub async fn session_profile_get(
    state: State<'_, AppState>,
    id: String,
) -> Result<Option<SessionProfile>, crate::error::AppError> {
    state
        .storage
        .get_profile(&id)
        .await
        .map_err(|e| crate::error::AppError::storage(format!("获取会话配置失败: {e}")))
}

/// 按 id 获取会话配置的敏感字段（密码 / 私钥口令）
///
/// 仅在用户发起连接时调用，避免敏感数据常驻前端内存。
#[tauri::command]
pub async fn session_profile_get_secret(
    _state: State<'_, AppState>,
    id: String,
) -> Result<Option<String>, crate::error::AppError> {
    keyring::get_credential(&keyring_key(&id)).map_err(|e| crate::error::AppError::storage(format!("读取 Keyring 失败: {e}")))
}

/// 删除会话配置（同时清理 SQLite + Keyring）
#[tauri::command]
pub async fn session_profile_delete(
    state: State<'_, AppState>,
    id: String,
) -> Result<bool, crate::error::AppError> {
    // 先删 SQLite
    let deleted = state
        .storage
        .delete_profile(&id)
        .await
        .map_err(|e| crate::error::AppError::storage(format!("删除会话配置失败: {e}")))?;

    // 再清 Keyring（即使 SQLite 删除失败也尝试清理凭据，避免残留）
    let _ = keyring::delete_credential(&keyring_key(&id));

    if deleted {
        tracing::info!(profile_id = %id, "会话配置已删除");
    }
    Ok(deleted)
}

/// 标记会话为最近使用（连接成功后调用，影响侧栏排序）
#[tauri::command]
pub async fn session_profile_touch(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), crate::error::AppError> {
    let now = chrono::Utc::now().timestamp();
    state
        .storage
        .touch_profile(&id, now)
        .await
        .map_err(|e| crate::error::AppError::storage(format!("更新 last_used_at 失败: {e}")))?;
    Ok(())
}
