//! # SSH 会话命令组
//!
//! 对接前端 SSH 终端交互：连接、输入、resize、断开。
//!
//! ## sessionId ↔ channel_id 关系
//! - `session_connect` 成功后，将 `sessionId → channel_id` 注册到 `AppState::channel_ids`
//! - `session_input` / `session_resize` 通过该映射查到 channel_id 后调用 SshSession
//! - `session_disconnect` 同时清理 channel_ids 与 ssh_manager 两个注册表

use tauri::{AppHandle, Emitter, State};

use crate::events::TerminalOutputPayload;
use crate::AppState;
use ssh_core::connection::ConnectionConfig;

/// 建立 SSH 会话
///
/// 成功返回 sessionId，前端据此建立 tab 并监听 `terminal_output` 事件。
///
/// 流程：
/// 1. `ssh_manager.connect` 建立 SSH 连接（TCP + 认证）
/// 2. `session.open_pty` 打开 PTY 通道
/// 3. `TerminalStream::start` 拉起读循环（背压 mpsc 容量 64）
/// 4. 注册 sessionId → channel_id 映射
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

    // 1) 建立 SSH 连接
    let session = state
        .ssh_manager
        .connect(&config)
        .await
        .map_err(|e| format!("{e}"))?;

    let session_id = session.id.clone();

    // 2) 打开 PTY + 3) 拉起读循环
    let pty = session.open_pty(cols, rows).await.map_err(|e| format!("{e}"))?;
    let channel_id = pty.channel_id;
    let mut stream = ssh_core::terminal::TerminalStream::start(pty);

    // 4) 注册 sessionId → channel_id 映射
    state
        .channel_ids
        .lock()
        .await
        .insert(session_id.clone(), channel_id);

    // 5) 拉起推送 task：从 mpsc 消费数据 → emit 事件
    //    channel_ids 清理由 session_disconnect 统一负责（避免跨 task 共享 Mutex）
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
        // channel 已关闭：drop stream 自动 abort 读循环
        // channel_ids / ssh_manager 的清理由 session_disconnect 统一处理
        stream.stop().await;
        drop(stream);
    });

    tracing::info!(
        session_id = %session_id,
        channel_id = %channel_id,
        "SSH 会话已建立"
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
/// 通过 sessionId 查 channel_id，调用 `SshSession::resize` 发送 window-change。
#[tauri::command]
pub async fn session_resize(
    state: State<'_, AppState>,
    session_id: String,
    cols: u32,
    rows: u32,
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
        .resize(channel_id, cols, rows)
        .await
        .map_err(|e| format!("{e}"))?;
    Ok(())
}

/// 断开 SSH 会话
///
/// 同时清理：
/// - `ssh_manager`：移除会话 + 主动 disconnect（读循环会因 channel 关闭自然退出）
/// - `channel_ids`：移除 channel_id 映射
#[tauri::command]
pub async fn session_disconnect(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<(), String> {
    // 清理 channel_id 映射
    state.channel_ids.lock().await.remove(&session_id);

    // 断开并移除会话
    state
        .ssh_manager
        .disconnect(&session_id)
        .await
        .map_err(|e| format!("{e}"))?;
    Ok(())
}

