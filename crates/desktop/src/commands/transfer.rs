//! # SFTP 传输命令组
//!
//! 文件/文件夹上传下载的队列管理（进度经 `transfer_event` 事件推送，
//! 见 [`crate::transfer::TransferManager`]）。

use tauri::{AppHandle, State};

use crate::error::AppError;
use crate::events::TransferEventPayload;
use crate::AppState;

use super::sftp::client_of;

/// 读取会话身份标签（"user@host:port"，传输列表展示用）
async fn session_label_of(state: &AppState, session_id: &str) -> Option<String> {
    state.session_meta.lock().await.get(session_id).cloned()
}

/// 上传本地文件/文件夹到远端目录（每个顶层条目一个传输组，返回组 ID）
#[tauri::command]
pub async fn sftp_transfer_upload(
    state: State<'_, AppState>,
    app: AppHandle,
    session_id: String,
    local_paths: Vec<String>,
    remote_dir: String,
) -> Result<Vec<String>, AppError> {
    let label = session_label_of(&state, &session_id).await;
    let client = client_of(&state, &session_id).await?;
    state
        .transfer_manager
        .add_upload(&app, &session_id, label, client, local_paths, remote_dir)
        .await
}

/// 下载远端文件/文件夹（save_dir 缺省为系统下载目录，重名自动去重）
#[tauri::command]
pub async fn sftp_transfer_download(
    state: State<'_, AppState>,
    app: AppHandle,
    session_id: String,
    remote_paths: Vec<String>,
    save_dir: Option<String>,
) -> Result<Vec<String>, AppError> {
    let label = session_label_of(&state, &session_id).await;
    let client = client_of(&state, &session_id).await?;
    state
        .transfer_manager
        .add_download(&app, &session_id, label, client, remote_paths, save_dir)
        .await
}

/// 暂停传输组（保留进度）
#[tauri::command]
pub async fn sftp_transfer_pause(
    state: State<'_, AppState>,
    app: AppHandle,
    group_id: String,
) -> Result<(), AppError> {
    state.transfer_manager.pause(&app, &group_id).await
}

/// 恢复暂停的传输组（断点续传）
#[tauri::command]
pub async fn sftp_transfer_resume(
    state: State<'_, AppState>,
    app: AppHandle,
    group_id: String,
) -> Result<(), AppError> {
    let session_id = state
        .transfer_manager
        .group_session(&group_id)
        .await
        .ok_or_else(|| AppError::session(format!("传输任务 {group_id} 不存在")))?;
    let client = client_of(&state, &session_id).await?;
    state.transfer_manager.resume(&app, &group_id, client).await
}

/// 取消传输组
#[tauri::command]
pub async fn sftp_transfer_cancel(
    state: State<'_, AppState>,
    app: AppHandle,
    group_id: String,
) -> Result<(), AppError> {
    state.transfer_manager.cancel(&app, &group_id).await
}

/// 重试失败/已取消的传输组（断点续传）
#[tauri::command]
pub async fn sftp_transfer_retry(
    state: State<'_, AppState>,
    app: AppHandle,
    group_id: String,
) -> Result<(), AppError> {
    let session_id = state
        .transfer_manager
        .group_session(&group_id)
        .await
        .ok_or_else(|| AppError::session(format!("传输任务 {group_id} 不存在")))?;
    let client = client_of(&state, &session_id).await?;
    state.transfer_manager.retry(&app, &group_id, client).await
}

/// 传输列表快照（可按会话过滤；前端初始化/恢复用）
#[tauri::command]
pub async fn sftp_transfers(
    state: State<'_, AppState>,
    session_id: Option<String>,
) -> Result<Vec<TransferEventPayload>, AppError> {
    Ok(state.transfer_manager.snapshot(session_id.as_deref()).await)
}

/// 清除已完成/失败/已取消的传输记录
#[tauri::command]
pub async fn sftp_transfers_clear(
    state: State<'_, AppState>,
    session_id: Option<String>,
) -> Result<(), AppError> {
    state.transfer_manager.clear_finished(session_id.as_deref()).await;
    Ok(())
}
