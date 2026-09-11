//! # SFTP 命令组
//!
//! 远端文件浏览 / 递归删除 / 增删改；上传下载走 commands::transfer 的传输队列。

use tauri::State;

use crate::error::AppError;
use crate::AppState;
use ssh_core::sftp::{SftpClient, SftpEntry};

/// 获取（或懒建立）指定会话的 SFTP 客户端（传输命令组也复用）
pub(crate) async fn client_of(state: &AppState, session_id: &str) -> Result<SftpClient, AppError> {
    // 先查缓存
    if let Some(c) = state.sftp_clients.lock().await.get(session_id) {
        return Ok(c.clone());
    }
    // 未命中：从 SSH 会话建立
    let session = state
        .ssh_manager
        .get(session_id)
        .await
        .ok_or_else(|| AppError::session(format!("会话 {session_id} 不存在或已断开")))?;
    let client = session
        .open_sftp()
        .await
        .map_err(|e| AppError::session(format!("初始化 SFTP 失败: {e}")))?;
    state
        .sftp_clients
        .lock()
        .await
        .insert(session_id.to_string(), client.clone());
    Ok(client)
}

/// 读取远端目录
#[tauri::command]
pub async fn sftp_list(
    state: State<'_, AppState>,
    session_id: String,
    path: Option<String>,
) -> Result<Vec<SftpEntry>, AppError> {
    let client = client_of(&state, &session_id).await?;
    let path = match path {
        Some(p) if !p.is_empty() => p,
        // 默认进入远端 home
        _ => client.home().await.map_err(|e| AppError::session(e.to_string()))?,
    };
    client.list(&path).await.map_err(|e| AppError::session(e.to_string()))
}

/// 获取远端路径元信息（用于存在性检查等；不存在时返回错误）
#[tauri::command]
pub async fn sftp_stat(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
) -> Result<SftpEntry, AppError> {
    let client = client_of(&state, &session_id).await?;
    client.stat(&path).await.map_err(|e| AppError::session(e.to_string()))
}

/// 创建目录
#[tauri::command]
pub async fn sftp_mkdir(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
) -> Result<(), AppError> {
    let client = client_of(&state, &session_id).await?;
    client.mkdir(&path).await.map_err(|e| AppError::session(e.to_string()))
}

/// 递归删除文件/目录（符号链接只删链接本身）
#[tauri::command]
pub async fn sftp_remove(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
) -> Result<(), AppError> {
    let client = client_of(&state, &session_id).await?;
    client
        .remove_recursive(&path)
        .await
        .map_err(|e| AppError::session(e.to_string()))
}

/// 重命名 / 移动
#[tauri::command]
pub async fn sftp_rename(
    state: State<'_, AppState>,
    session_id: String,
    old_path: String,
    new_path: String,
) -> Result<(), AppError> {
    let client = client_of(&state, &session_id).await?;
    client
        .rename(&old_path, &new_path)
        .await
        .map_err(|e| AppError::session(e.to_string()))
}

/// 修改文件/目录权限（chmod，mode 为八进制位如 0o755）
#[tauri::command]
pub async fn sftp_chmod(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
    mode: u32,
) -> Result<(), AppError> {
    let client = client_of(&state, &session_id).await?;
    client
        .chmod(&path, mode)
        .await
        .map_err(|e| AppError::session(e.to_string()))
}
