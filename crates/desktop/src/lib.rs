//! # SmileX Desktop 应用库
//!
//! 整合 SSH（ssh-core）、远程桌面（remote-desktop-core）、AI 助手（ai-core），
//! 暴露 Tauri commands 给前端调用。

pub mod commands;
pub mod events;
pub mod storage;

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

use ssh_core::connection::SessionManager as SshSessionManager;
use ssh_core::terminal::TerminalControl;
use storage::sqlite::SqliteStorage;

/// 应用全局状态（Tauri managed state）
///
/// 在 Tauri 启动时通过 `.manage(AppState::new())` 注册，
/// 所有 commands 通过 `tauri::State<AppState>` 访问。
pub struct AppState {
    /// SSH 会话管理器（sessionId → SshSession）
    pub ssh_manager: Arc<SshSessionManager>,
    /// sessionId → PTY channel_id 映射（用于 session_input）
    ///
    /// 注：保留 russh 原生 `ChannelId` 类型，避免 u32 ↔ ChannelId 不可逆转换。
    pub channel_ids: Mutex<HashMap<String, russh::ChannelId>>,
    /// sessionId → 终端控制句柄（用于 session_resize）
    ///
    /// **阶段 4 新增**：解决 russh 0.45 `Channel::window_change(&self)` 与
    /// `Channel::wait(&mut self)` 互斥问题，通过 TerminalControl 投递指令到
    /// select! 读循环内串行执行。
    pub terminal_controls: Mutex<HashMap<String, TerminalControl>>,
    /// AI Chat 提供者
    pub chat_provider: Arc<ai_core::provider::chat::ChatProvider>,
    /// 远程桌面会话注册表（阶段 3 完整实现）
    pub desktop_sessions:
        Mutex<HashMap<String, Box<dyn remote_desktop_core::session::RemoteDesktopSession>>>,
    /// SQLite 存储（会话配置持久化）
    pub storage: Arc<SqliteStorage>,
}

impl AppState {
    /// 创建应用状态（需传入已打开的 SQLite 句柄）
    pub fn new(storage: SqliteStorage) -> Self {
        Self {
            ssh_manager: Arc::new(SshSessionManager::new()),
            channel_ids: Mutex::new(HashMap::new()),
            terminal_controls: Mutex::new(HashMap::new()),
            chat_provider: Arc::new(ai_core::provider::chat::ChatProvider::new()),
            desktop_sessions: Mutex::new(HashMap::new()),
            storage: Arc::new(storage),
        }
    }

    /// 应用退出时清理所有会话
    pub async fn cleanup(&self) {
        tracing::info!("应用退出，清理所有会话...");
        self.ssh_manager.disconnect_all().await;
        self.channel_ids.lock().await.clear();
        // drop TerminalControl 触发读循环退出
        self.terminal_controls.lock().await.clear();
        // 远程桌面会话清理
        let mut sessions = self.desktop_sessions.lock().await;
        for (_, session) in sessions.drain() {
            let _ = session.disconnect().await;
        }
        tracing::info!("会话清理完成");
    }
}

