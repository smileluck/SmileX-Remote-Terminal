//! # SSH 会话命令组
//!
//! 对接前端 SSH 终端交互：连接、输入、resize、断开。

use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

use crate::events::TerminalOutputPayload;
use crate::AppState;
use ssh_core::connection::ConnectionConfig;

/// 建立 SSH 会话
///
/// 成功返回 sessionId，前端据此建立 tab 并监听 `terminal_output` 事件。
#[tauri::command]
pub async fn session_connect(
    app: AppHandle,
    state: State<'_, AppState>,
    config: ConnectionConfig,
    cols: Option<u32>,
    rows: Option<u32>,
) -> Result<String, String> {
    let cols = cols.unwrap_or(80);
    let rows = rows.unwrap_or(24);

    // 建立 SSH 连接
    let session = state
        .ssh_manager
        .connect(&config)
        .await
        .map_err(|e| format!("{e}"))?;

    let session_id = session.id.clone();

    // 打开 PTY 并拉起读循环
    let pty = session
        .open_pty(cols, rows)
        .await
        .map_err(|e| format!("{e}"))?;

    let channel_id = pty.channel_id;
    let mut stream = ssh_core::terminal::TerminalStream::start(pty);

    // 拉起推送 task：从 mpsc 消费数据 → emit 事件
    let app_clone = app.clone();
    let session_id_clone = session_id.clone();
    tokio::spawn(async move {
        while let Some(chunk) = stream.rx.recv().await {
            let payload = TerminalOutputPayload {
                session_id: session_id_clone.clone(),
                data: chunk.data,
            };
            // 单条事件推送（阶段 3 在此加节流批处理，详见架构 §2.6）
            if app_clone.emit("terminal_output", payload).is_err() {
                break;
            }
        }
        // channel 已关闭，保持 stream 的 join handle 被 drop 时自动 abort
        stream.stop().await;
        drop(stream);
    });

    // 保存 channel_id 到 state（用于 session_input）
    // MVP 阶段：用简单 HashMap 存 sessionId → channel_id
    // 注：这里简化为每次输入直接用 session.write，需要 channel_id 映射
    // 完整实现在 commands/registry.rs 统一管理
    tracing::info!(session_id = %session_id, channel_id = channel_id, "SSH 会话已建立");

    Ok(session_id)
}

/// 终端输入（用户键盘输入）
#[tauri::command]
pub async fn session_input(
    state: State<'_, AppState>,
    session_id: String,
    data: Vec<u8>,
) -> Result<(), String> {
    let session = state
        .ssh_manager
        .get(&session_id)
        .await
        .ok_or_else(|| "会话不存在".to_string())?;
    // MVP：channel_id 映射待完善，暂用 0 占位
    // 阶段 3 在 registry 中统一维护 sessionId → channel_id 映射
    session
        .write(0u32, &data)
        .await
        .map_err(|e| format!("{e}"))?;
    Ok(())
}

/// 终端尺寸变更
#[tauri::command]
pub async fn session_resize(
    _state: State<'_, AppState>,
    _session_id: String,
    _cols: u32,
    _rows: u32,
) -> Result<(), String> {
    // 阶段 3 实现：通过 russh 发 window-change 请求
    Ok(())
}

/// 断开 SSH 会话
#[tauri::command]
pub async fn session_disconnect(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<(), String> {
    state
        .ssh_manager
        .disconnect(&session_id)
        .await
        .map_err(|e| format!("{e}"))?;
    Ok(())
}

