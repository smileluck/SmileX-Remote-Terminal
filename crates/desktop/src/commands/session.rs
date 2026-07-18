//! # SSH 会话命令组
//!
//! 对接前端 SSH 终端交互：连接、输入、resize、断开。
//!
//! ## sessionId ↔ channel_id 关系
//! - `session_connect` 成功后，将 `sessionId → channel_id` 注册到 `AppState::channel_ids`
//! - 同时将 `sessionId → TerminalControl` 注册到 `AppState::terminal_controls`（阶段 4）
//! - `session_input` 通过 channel_ids 查到 channel_id 后调用 `SshSession::write`
//! - `session_resize` 通过 terminal_controls 投递 Resize 指令到读循环 select!
//! - `session_disconnect` 同时清理 channel_ids / terminal_controls / ssh_manager
//!
//! ## host key 校验（阶段 4）
//! `session_connect` 从 AppState 取出 SQLite storage（实现了 `KnownHostsStore`），
//! 注入到 `SshSession::connect`，由 `HostKeyHandler` 在握手阶段校验。

use std::sync::Arc;

use tauri::{AppHandle, Emitter, State};

use crate::events::TerminalOutputPayload;
use crate::AppState;
use ssh_core::connection::ConnectionConfig;
use ssh_core::known_hosts::KnownHostsStore;

/// 建立 SSH 会话
///
/// 成功返回 sessionId，前端据此建立 tab 并监听 `terminal_output` 事件。
///
/// 流程：
/// 1. `ssh_manager.connect` 建立 SSH 连接（TCP + 认证）
/// 2. `session.open_pty` 打开 PTY 通道
/// 3. `TerminalStream::start` 拉起 select! 读循环（背压 mpsc 容量 64 + 控制通道 16）
/// 4. 注册 sessionId → channel_id / TerminalControl 映射
/// 5. 拉起推送 task：mpsc rx → Tauri event `terminal_output`
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

    // 0) 注入 known_hosts 存储（SqliteStorage 实现了 KnownHostsStore trait）
    //    clone Arc 廉价，后续在 HostKeyHandler::check_server_key 中查询/保存
    let known_hosts: Arc<dyn KnownHostsStore> = state.storage.clone();

    // 1) 建立 SSH 连接（含 host key 校验）
    let session = state
        .ssh_manager
        .connect(&config, known_hosts)
        .await
        .map_err(|e| format!("{e}"))?;

    let session_id = session.id.clone();

    // 2) 打开 PTY + 3) 拉起 select! 读循环
    let pty = session.open_pty(cols, rows).await.map_err(|e| format!("{e}"))?;
    let channel_id = pty.channel_id;
    let (mut stream, control) = ssh_core::terminal::TerminalStream::start(pty);

    // 4) 注册 sessionId → channel_id / TerminalControl 映射
    state
        .channel_ids
        .lock()
        .await
        .insert(session_id.clone(), channel_id);
    state
        .terminal_controls
        .lock()
        .await
        .insert(session_id.clone(), control);

    // 5) 拉起推送 task：从 mpsc 消费数据 → emit 事件
    //    channel_ids / terminal_controls 清理由 session_disconnect 统一负责
    let app_clone = app.clone();
    let session_id_clone = session_id.clone();
    tokio::spawn(async move {
        while let Some(chunk) = stream.rx.recv().await {
            let payload = TerminalOutputPayload {
                session_id: session_id_clone.clone(),
                data: chunk.data,
            };
            if app_clone.emit("terminal_output", payload).is_err() {
                break;
            }
        }
        // channel 已关闭：drop stream 自动 abort 读循环
        stream.stop().await;
        drop(stream);
    });

    tracing::info!(
        session_id = %session_id,
        channel_id = %channel_id,
        "SSH 会话已建立（含 TerminalControl）"
    );

    Ok(session_id)
}

/// 终端输入（用户键盘输入）
///
/// 通过 sessionId 查 channel_id，再调用 `SshSession::write` 写入。
#[tauri::command]
pub async fn session_input(
    state: State<'_, AppState>,
    session_id: String,
    data: Vec<u8>,
) -> Result<(), String> {
    // 查 channel_id
    let channel_id = state
        .channel_ids
        .lock()
        .await
        .get(&session_id)
        .copied()
        .ok_or_else(|| format!("会话 {session_id} 未注册 channel_id"))?;

    // 查 session
    let session = state
        .ssh_manager
        .get(&session_id)
        .await
        .ok_or_else(|| "会话不存在".to_string())?;

    session
        .write(channel_id, &data)
        .await
        .map_err(|e| format!("{e}"))?;
    Ok(())
}

/// 终端尺寸变更（前端 xterm.js resize 触发）
///
/// 通过 sessionId 查 TerminalControl，投递 Resize 指令到读循环 select!。
/// 读循环在分支内串行调用 `channel.window_change(cols, rows)`，
/// 避免 russh 0.45 `&self/&mut self` 互斥问题。
#[tauri::command]
pub async fn session_resize(
    state: State<'_, AppState>,
    session_id: String,
    cols: u32,
    rows: u32,
) -> Result<(), String> {
    // 查 TerminalControl
    let control = state
        .terminal_controls
        .lock()
        .await
        .get(&session_id)
        .cloned()
        .ok_or_else(|| format!("会话 {session_id} 未注册 TerminalControl"))?;

    // 投递 Resize 指令（读循环已退出时返回错误）
    control
        .resize(cols, rows)
        .await
        .map_err(|e| format!("{e}"))?;
    Ok(())
}

/// 断开 SSH 会话
///
/// 同时清理三个注册表：
/// - `ssh_manager`：移除会话 + 主动 disconnect（读循环会因 channel 关闭自然退出）
/// - `channel_ids`：移除 channel_id 映射
/// - `terminal_controls`：移除 TerminalControl（drop 后读循环控制通道关闭，加速退出）
#[tauri::command]
pub async fn session_disconnect(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<(), String> {
    // 清理 channel_id 映射
    state.channel_ids.lock().await.remove(&session_id);
    // 清理 TerminalControl（drop 后读循环 ctrl_rx.recv() 返回 None 退出）
    state.terminal_controls.lock().await.remove(&session_id);

    // 断开并移除会话
    state
        .ssh_manager
        .disconnect(&session_id)
        .await
        .map_err(|e| format!("{e}"))?;
    Ok(())
}

