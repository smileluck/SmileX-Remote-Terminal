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
use std::sync::atomic::Ordering;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::events::TerminalOutputPayload;
use crate::{AppState, TerminalStats};
use ssh_core::connection::ConnectionConfig;
use ssh_core::known_hosts::KnownHostsStore;

/// 在指定会话的独立通道上执行一次性命令，返回 stdout
///
/// 用于命令补全（compgen）等非交互场景，不影响 PTY 数据流。
#[tauri::command]
pub async fn session_exec(
    state: State<'_, AppState>,
    session_id: String,
    command: String,
) -> Result<String, crate::error::AppError> {
    let session = state
        .ssh_manager
        .get(&session_id)
        .await
        .ok_or_else(|| crate::error::AppError::session(format!("会话 {session_id} 不存在或已断开")))?;
    session
        .exec(&command)
        .await
        .map_err(|e| crate::error::AppError::session(format!("{e}")))
}

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
    on_output: tauri::ipc::Channel<TerminalOutputPayload>,
) -> Result<String, crate::error::AppError> {
    let cols = cols.unwrap_or(80);
    let rows = rows.unwrap_or(24);

    // 0) 注入 known_hosts 存储（SqliteStorage 实现了 KnownHostsStore trait）
    //    clone Arc 廉价，后续在 HostKeyHandler::check_server_key 中查询/保存
    let known_hosts: Arc<dyn KnownHostsStore> = state.storage.clone();

    // 0.5) 注入未知主机交互式确认回调：emit `hostkey_confirm` → 前端弹窗
    //      → 前端 invoke `host_key_respond` → oneshot 唤醒握手流程（60s 超时）
    let confirm_app = app.clone();
    let confirm_awaits = state.host_key_awaits.clone();
    let confirm: Option<ssh_core::connection::HostKeyConfirm> = Some(Arc::new(
        move |challenge: ssh_core::connection::HostKeyChallenge| {
            let app = confirm_app.clone();
            let awaits = confirm_awaits.clone();
            Box::pin(async move {
                let request_id = uuid::Uuid::new_v4().to_string();
                let (tx, rx) = tokio::sync::oneshot::channel::<bool>();
                awaits.lock().await.insert(request_id.clone(), tx);

                let payload = serde_json::json!({
                    "request_id": request_id,
                    "host": challenge.host,
                    "port": challenge.port,
                    "key_type": challenge.key_type,
                    "fingerprint": challenge.fingerprint,
                });
                tracing::info!(
                    host = %challenge.host,
                    port = challenge.port,
                    request_id = %request_id,
                    "等待用户确认 host key"
                );
                if app.emit("hostkey_confirm", payload).is_err() {
                    awaits.lock().await.remove(&request_id);
                    return false;
                }

                // 60s 未应答视为拒绝，避免握手无限挂起
                let accepted = tokio::time::timeout(
                    std::time::Duration::from_secs(60),
                    rx,
                )
                .await
                .map(|r| r.unwrap_or(false))
                .unwrap_or(false);
                awaits.lock().await.remove(&request_id);
                accepted
            })
        },
    ));

    // 1) 建立 SSH 连接（含 host key 校验）
    let session = state
        .ssh_manager
        .connect(&config, known_hosts, confirm)
        .await
        .map_err(|e| crate::error::AppError::session(format!("{e}")))?;

    let session_id = session.id.clone();

    // 2) 打开 PTY + 3) 拉起 select! 读循环
    let pty = session.open_pty(cols, rows).await.map_err(|e| crate::error::AppError::session(format!("{e}")))?;
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

    // 注册流量统计（含连接时间戳，供会话健康面板使用）
    let stats = Arc::new(TerminalStats::now());
    state
        .terminal_stats
        .lock()
        .await
        .insert(session_id.clone(), stats.clone());

    // 注册服务器身份（AI 上下文注入用：user@host:port）
    state
        .session_meta
        .lock()
        .await
        .insert(session_id.clone(), format!("{}@{}:{}", config.username, config.host, config.port));

    // 5) 拉起推送 task：从 mpsc 消费数据 → 经 ipc::Channel 点对点推送
    //    （相比全局 emit，避免高频终端输出的跨窗口广播与重复序列化）
    //    channel_ids / terminal_controls 清理由 session_disconnect 统一负责
    //    同时累积终端回滚缓冲（AI 上下文注入用，容量上限 ~16KB）
    let scrollback = Arc::new(tokio::sync::Mutex::new(String::new()));
    state
        .terminal_scrollback
        .lock()
        .await
        .insert(session_id.clone(), scrollback.clone());
    const SCROLLBACK_CAP: usize = 16 * 1024;
    let session_id_clone = session_id.clone();
    tokio::spawn(async move {
        while let Some(chunk) = stream.rx.recv().await {
            stats.rx_bytes.fetch_add(chunk.data.len() as u64, Ordering::Relaxed);
            // 追加回滚缓冲（超限时丢弃最旧内容）
            {
                let mut sb = scrollback.lock().await;
                sb.push_str(&String::from_utf8_lossy(&chunk.data));
                if sb.len() > SCROLLBACK_CAP {
                    let cut = sb.len() - SCROLLBACK_CAP;
                    let start = sb
                        .char_indices()
                        .map(|(i, _)| i)
                        .find(|&i| i >= cut)
                        .unwrap_or(sb.len());
                    *sb = sb[start..].to_string();
                }
            }
            let payload = TerminalOutputPayload {
                session_id: session_id_clone.clone(),
                data: chunk.data,
            };
            if on_output.send(payload).is_err() {
                break;
            }
        }
        // channel 已关闭：drop stream 自动 abort 读循环
        stream.stop().await;
        drop(stream);
        // 通知前端会话已断开（tab 标记 + 通知 + 提供重连入口）
        let _ = app.emit(
            "session_closed",
            serde_json::json!({ "session_id": session_id_clone }),
        );
    });

    tracing::info!(
        session_id = %session_id,
        channel_id = %channel_id,
        "SSH 会话已建立（含 TerminalControl）"
    );

    Ok(session_id)
}

/// 前端 host key 确认弹窗的应答
///
/// 配合 `hostkey_confirm` 事件：用户选择「信任」/「拒绝」后调用，
/// 唤醒被挂起的 SSH 握手流程。
#[tauri::command]
pub async fn host_key_respond(
    state: State<'_, AppState>,
    request_id: String,
    accept: bool,
) -> Result<(), crate::error::AppError> {
    if let Some(tx) = state.host_key_awaits.lock().await.remove(&request_id) {
        let _ = tx.send(accept);
    }
    Ok(())
}

/// 终端输入（用户键盘输入）
///
/// 通过 sessionId 查 channel_id，再调用 `SshSession::write` 写入。
#[tauri::command]
pub async fn session_input(
    state: State<'_, AppState>,
    session_id: String,
    data: Vec<u8>,
) -> Result<(), crate::error::AppError> {
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
        .map_err(|e| crate::error::AppError::session(format!("{e}")))?;

    // 累计上行流量（统计句柄可能已被清理，忽略即可）
    if let Some(stats) = state.terminal_stats.lock().await.get(&session_id) {
        stats.tx_bytes.fetch_add(data.len() as u64, Ordering::Relaxed);
    }
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
) -> Result<(), crate::error::AppError> {
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
        .map_err(|e| crate::error::AppError::session(format!("{e}")))?;
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
    app: tauri::AppHandle,
    session_id: String,
) -> Result<(), crate::error::AppError> {
    // 停止该会话的监控采样
    state.monitor_sampler.stop(&session_id).await;
    // 取消该会话所有未完成传输（传输依赖的 SSH 通道即将关闭）
    state
        .transfer_manager
        .cancel_session(Some(&app), &session_id)
        .await;
    // 清理 channel_id 映射
    state.channel_ids.lock().await.remove(&session_id);
    // 清理 TerminalControl（drop 后读循环 ctrl_rx.recv() 返回 None 退出）
    state.terminal_controls.lock().await.remove(&session_id);
    // 清理流量统计
    state.terminal_stats.lock().await.remove(&session_id);
    // 清理 SFTP 客户端缓存
    state.sftp_clients.lock().await.remove(&session_id);
    // 清理终端回滚缓冲
    state.terminal_scrollback.lock().await.remove(&session_id);
    // 清理服务器身份
    state.session_meta.lock().await.remove(&session_id);

    // 断开并移除会话
    state
        .ssh_manager
        .disconnect(&session_id)
        .await
        .map_err(|e| crate::error::AppError::session(format!("{e}")))?;
    Ok(())
}

/// 单会话健康信息（`session_stats_all` 返回项）
#[derive(Debug, Clone, Serialize)]
pub struct SessionHealth {
    pub session_id: String,
    /// 是否仍连接（存在于 ssh_manager）
    pub connected: bool,
    /// 连接建立时间戳（毫秒）
    pub connected_at_ms: u64,
    /// 下行累计字节
    pub rx_bytes: u64,
    /// 上行累计字节
    pub tx_bytes: u64,
}

/// 所有会话的健康/流量信息（右栏会话健康面板轮询）
#[tauri::command]
pub async fn session_stats_all(state: State<'_, AppState>) -> Result<Vec<SessionHealth>, crate::error::AppError> {
    let stats_map = state.terminal_stats.lock().await.clone();
    let mut result = Vec::with_capacity(stats_map.len());
    for (session_id, stats) in stats_map {
        let connected = state.ssh_manager.get(&session_id).await.is_some();
        result.push(SessionHealth {
            session_id,
            connected,
            connected_at_ms: stats.connected_at_ms.load(Ordering::Relaxed),
            rx_bytes: stats.rx_bytes.load(Ordering::Relaxed),
            tx_bytes: stats.tx_bytes.load(Ordering::Relaxed),
        });
    }
    Ok(result)
}

