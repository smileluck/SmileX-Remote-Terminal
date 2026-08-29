//! # 远程桌面命令组
//!
//! 对接前端远程桌面：连接（按 platform 路由 RDP/Host）、输入、断开。

use tauri::{AppHandle, Emitter, State};

use crate::events::DesktopFramePayload;
use crate::AppState;
use remote_desktop_core::session::SessionConfig;
use remote_desktop_core::InputEvent;

/// 建立远程桌面会话
#[tauri::command]
pub async fn desktop_connect(
    app: AppHandle,
    state: State<'_, AppState>,
    config: SessionConfig,
) -> Result<String, crate::error::AppError> {
    let session_id = uuid::Uuid::new_v4().to_string();

    // 创建会话实例（按 config.kind 路由）
    let mut session = remote_desktop_core::session::create_session(
        session_id.clone(),
        config.kind,
    );

    // 启动会话，获取帧接收端
    let mut rx = session.start(&config).await.map_err(|e| crate::error::AppError::desktop(format!("{e}")))?;

    // 注册到 state
    state
        .desktop_sessions
        .lock()
        .await
        .insert(session_id.clone(), session);

    let session_id_clone = session_id.clone();
    let app_clone = app.clone();

    // 拉起帧推送 task：rx → emit `desktop_frame`
    tokio::spawn(async move {
        while let Some(frame) = rx.recv().await {
            let payload = DesktopFramePayload {
                session_id: session_id_clone.clone(),
                seq: frame.seq,
                width: frame.width,
                height: frame.height,
                rgba: frame.rgba,
                key_frame: frame.key_frame,
            };
            if app_clone.emit("desktop_frame", payload).is_err() {
                break;
            }
        }
    });

    Ok(session_id)
}

/// 发送输入事件（鼠标/键盘）
#[tauri::command]
pub async fn desktop_input(
    state: State<'_, AppState>,
    session_id: String,
    event: InputEvent,
) -> Result<(), crate::error::AppError> {
    let sessions = state.desktop_sessions.lock().await;
    let session = sessions
        .get(&session_id)
        .ok_or_else(|| "会话不存在".to_string())?;
    session
        .send_input(event)
        .await
        .map_err(|e| crate::error::AppError::desktop(format!("{e}")))?;
    Ok(())
}

/// 断开远程桌面会话
#[tauri::command]
pub async fn desktop_disconnect(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<(), crate::error::AppError> {
    let mut sessions = state.desktop_sessions.lock().await;
    if let Some(session) = sessions.remove(&session_id) {
        session
            .disconnect()
            .await
            .map_err(|e| crate::error::AppError::desktop(format!("{e}")))?;
    }
    Ok(())
}
