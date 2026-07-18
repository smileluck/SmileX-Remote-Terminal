//! # SmileX Desktop 应用库
//!
//! 整合 SSH（ssh-core）、远程桌面（remote-desktop-core）、AI 助手（ai-core），
//! 暴露 Tauri commands 给前端调用。

pub mod commands;
pub mod events;
pub mod storage;

use std::sync::Arc;
use tokio::sync::Mutex;

use ssh_core::connection::SessionManager as SshSessionManager;

/// 应用全局状态（Tauri managed state）
///
/// 在 Tauri 启动时通过 `.manage(AppState::new())` 注册，
/// 所有 commands 通过 `tauri::State<AppState>` 访问。
pub struct AppState {
    /// SSH 会话管理器
    pub ssh_manager: Arc<SshSessionManager>,
    /// AI Chat 提供者
    pub chat_provider: Arc<ai_core::provider::chat::ChatProvider>,
    /// 远程桌面会话注册表（阶段 3 完整实现）
    pub desktop_sessions: Mutex<std::collections::HashMap<String, Box<dyn remote_desktop_core::session::RemoteDesktopSession>>>,
}

impl AppState {
    /// 创建应用状态
    pub fn new() -> Self {
        Self {
            ssh_manager: Arc::new(SshSessionManager::new()),
            chat_provider: Arc::new(ai_core::provider::chat::ChatProvider::new()),
            desktop_sessions: Mutex::new(std::collections::HashMap::new()),
        }
    }

    /// 应用退出时清理所有会话
    pub async fn cleanup(&self) {
        tracing::info!("应用退出，清理所有会话...");
        self.ssh_manager.disconnect_all().await;
        // 远程桌面会话清理
        let mut sessions = self.desktop_sessions.lock().await;
        for (_, session) in sessions.drain() {
            let _ = session.disconnect().await;
        }
        tracing::info!("会话清理完成");
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
