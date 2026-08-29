//! # 监控命令组
//!
//! 启停按会话的指标采样（数据经 `monitor_metrics` 事件推送）。

use tauri::{AppHandle, State};

use crate::AppState;

/// 启动某会话的监控采样
///
/// 前端在 SSH 会话建立（或激活监控）后调用；
/// 数据通过 `monitor_metrics` 事件按 `interval_ms` 周期推送。
#[tauri::command]
pub async fn monitor_start(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    interval_ms: Option<u64>,
) -> Result<(), crate::error::AppError> {
    // 会话必须存在（未连接时报错给前端）
    if state.ssh_manager.get(&session_id).await.is_none() {
        return Err(crate::error::AppError::monitor(format!("会话 {session_id} 不存在或已断开")));
    }
    state
        .monitor_sampler
        .start(app, state.ssh_manager.clone(), session_id, interval_ms.unwrap_or(3000))
        .await;
    Ok(())
}

/// 停止某会话的监控采样
#[tauri::command]
pub async fn monitor_stop(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<(), crate::error::AppError> {
    state.monitor_sampler.stop(&session_id).await;
    Ok(())
}

/// 当前采样中的会话列表
#[tauri::command]
pub async fn monitor_list(state: State<'_, AppState>) -> Result<Vec<String>, crate::error::AppError> {
    Ok(state.monitor_sampler.list().await)
}
