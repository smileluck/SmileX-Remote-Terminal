//! # SFTP 命令组
//!
//! 远端文件浏览 / 上传 / 下载 / 增删改。
//! SFTP 客户端按 sessionId 懒初始化并缓存（复用同一通道）。

use tauri::State;

use crate::error::AppError;
use crate::AppState;
use ssh_core::sftp::{SftpClient, SftpEntry};

/// 获取（或懒建立）指定会话的 SFTP 客户端
async fn client_of(state: &AppState, session_id: &str) -> Result<SftpClient, AppError> {
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

/// 上传文件（前端传 ArrayBuffer/Uint8Array → Vec<u8>）
#[tauri::command]
pub async fn sftp_upload(
    state: State<'_, AppState>,
    session_id: String,
    remote_path: String,
    data: Vec<u8>,
) -> Result<(), AppError> {
    let client = client_of(&state, &session_id).await?;
    client
        .write(&remote_path, data)
        .await
        .map_err(|e| AppError::session(e.to_string()))
}

/// 下载文件：保存到本地 ~/Downloads（返回保存路径）
#[tauri::command]
pub async fn sftp_download(
    state: State<'_, AppState>,
    session_id: String,
    remote_path: String,
) -> Result<String, AppError> {
    let client = client_of(&state, &session_id).await?;
    let data = client
        .read(&remote_path)
        .await
        .map_err(|e| AppError::session(e.to_string()))?;

    let file_name = remote_path.rsplit('/').next().unwrap_or("download.bin");
    let downloads = dirs::download_dir()
        .or_else(dirs::home_dir)
        .ok_or_else(|| AppError::new(crate::error::ErrorCode::Internal, "无法定位本地下载目录"))?;
    let local_path = downloads.join(file_name);

    tokio::fs::write(&local_path, &data)
        .await
        .map_err(|e| AppError::new(crate::error::ErrorCode::Internal, format!("写入本地文件失败: {e}")))?;

    Ok(local_path.to_string_lossy().into_owned())
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

/// 删除文件或空目录
#[tauri::command]
pub async fn sftp_remove(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
    is_dir: bool,
) -> Result<(), AppError> {
    let client = client_of(&state, &session_id).await?;
    let res = if is_dir {
        client.remove_dir(&path).await
    } else {
        client.remove_file(&path).await
    };
    res.map_err(|e| AppError::session(e.to_string()))
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
