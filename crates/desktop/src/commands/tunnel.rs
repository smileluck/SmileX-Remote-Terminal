//! # 端口转发命令组
//!
//! 对接前端隧道管理面板：启动 / 停止 / 列出当前会话的 SSH 端口转发隧道
//! （Local / Remote / Dynamic，实现见 ssh-core::tunnel）。
//!
//! 状态变更通过全局事件 `tunnel_event`（[`TunnelEventPayload`]）推送：
//! - 启动成功 / 停止：由本模块在命令路径上 emit
//! - 运行期失败（如 remote 转发因会话断开失效）：由注入 TunnelManager 的
//!   notify 回调在后台 task 中 emit

use std::sync::Arc;

use tauri::{AppHandle, Emitter, State};

use crate::error::AppError;
use crate::events::TunnelEventPayload;
use crate::AppState;
use ssh_core::tunnel::{TunnelConfig, TunnelInfo, TunnelNotify, TunnelStatus};

/// 构造隧道状态变更通知回调：emit `tunnel_event` → 前端刷新隧道列表
fn tunnel_notify<R: tauri::Runtime>(app: &AppHandle<R>) -> TunnelNotify {
    let app = app.clone();
    Arc::new(move |tunnel_id: String, session_id: String, status: TunnelStatus| {
        let _ = app.emit(
            "tunnel_event",
            TunnelEventPayload {
                tunnel_id,
                session_id,
                state: status.as_str().to_string(),
                error: match &status {
                    TunnelStatus::Failed(e) => Some(e.clone()),
                    _ => None,
                },
            },
        );
    })
}

/// 启动隧道，返回 tunnel_id
///
/// `config.id` 为空时由后端生成 UUID（档案自动启动场景由前端预生成，
/// 以便面板把运行中隧道与档案配置对应标注来源）。
/// 监听绑定 / tcpip-forward 请求失败时直接返回错误（隧道未建立）。
#[tauri::command]
pub async fn tunnel_start<R: tauri::Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    session_id: String,
    config: TunnelConfig,
) -> Result<String, AppError> {
    let session = state
        .ssh_manager
        .get(&session_id)
        .await
        .ok_or_else(|| AppError::session(format!("会话 {session_id} 不存在或已断开")))?;

    let notify = tunnel_notify(&app);
    let tunnel_id = state
        .tunnels
        .start(session, config, Some(notify))
        .await
        .map_err(|e| AppError::session(format!("{e}")))?;

    let _ = app.emit(
        "tunnel_event",
        TunnelEventPayload {
            tunnel_id: tunnel_id.clone(),
            session_id,
            state: "running".to_string(),
            error: None,
        },
    );
    Ok(tunnel_id)
}

/// 停止隧道（幂等：不存在的 id 视为成功）
#[tauri::command]
pub async fn tunnel_stop<R: tauri::Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    tunnel_id: String,
) -> Result<(), AppError> {
    // 先查出 session_id 供事件携带（stop 后句柄已移除）
    let session_id = state
        .tunnels
        .get_info(&tunnel_id)
        .await
        .map(|t| t.session_id);
    state
        .tunnels
        .stop(&tunnel_id)
        .await
        .map_err(|e| AppError::session(format!("{e}")))?;
    if let Some(session_id) = session_id {
        let _ = app.emit(
            "tunnel_event",
            TunnelEventPayload {
                tunnel_id,
                session_id,
                state: "stopped".to_string(),
                error: None,
            },
        );
    }
    Ok(())
}

/// 列出指定会话的隧道（含运行状态，面板刷新用）
#[tauri::command]
pub async fn tunnel_list(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<Vec<TunnelInfo>, AppError> {
    Ok(state.tunnels.list(&session_id).await)
}
