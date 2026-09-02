//! # SmileX Desktop 应用库
//!
//! 整合 SSH（ssh-core）、远程桌面（remote-desktop-core）、AI 助手（ai-core），
//! 暴露 Tauri commands 给前端调用。

pub mod commands;
pub mod error;
pub mod events;
pub mod monitor;
pub mod storage;

use std::collections::HashMap;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use tokio::sync::Mutex;

use ssh_core::connection::SessionManager as SshSessionManager;
use ssh_core::terminal::TerminalControl;
use storage::sqlite::SqliteStorage;

/// 单会话终端流量统计（bytes，Atomic 原子累加）
#[derive(Debug, Default)]
pub struct TerminalStats {
    /// 终端下行（远端 → 本地）累计字节
    pub rx_bytes: AtomicU64,
    /// 终端上行（本地 → 远端）累计字节
    pub tx_bytes: AtomicU64,
    /// 连接建立时间戳（毫秒，用于会话时长展示）
    pub connected_at_ms: AtomicU64,
}

impl TerminalStats {
    /// 创建并记录当前时间为连接时间
    pub fn now() -> Self {
        Self {
            connected_at_ms: AtomicU64::new(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(0),
            ),
            ..Default::default()
        }
    }
}

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
    /// 监控采样调度器（sessionId → 定时采集 task）
    pub monitor_sampler: Arc<monitor::sampler::MonitorSampler>,
    /// sessionId → 终端流量统计
    pub terminal_stats: Mutex<HashMap<String, Arc<TerminalStats>>>,
    /// host key 确认等待表：requestId → oneshot 应答通道
    /// （Arc 包装：交互式确认回调需跨 task 持有）
    pub host_key_awaits: Arc<Mutex<HashMap<String, tokio::sync::oneshot::Sender<bool>>>>,
    /// sessionId → SFTP 客户端（懒初始化，断开会话时移除）
    pub sftp_clients: Mutex<HashMap<String, ssh_core::sftp::SftpClient>>,
    /// sessionId → 终端回滚缓冲（AI 上下文注入用，约最近 200 行）
    pub terminal_scrollback: Mutex<HashMap<String, Arc<Mutex<String>>>>,
    /// sessionId → 服务器身份（"user@host:port"，AI 上下文注入用）
    pub session_meta: Mutex<HashMap<String, String>>,
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
            monitor_sampler: Arc::new(monitor::sampler::MonitorSampler::new()),
            terminal_stats: Mutex::new(HashMap::new()),
            host_key_awaits: Arc::new(Mutex::new(HashMap::new())),
            sftp_clients: Mutex::new(HashMap::new()),
            terminal_scrollback: Mutex::new(HashMap::new()),
            session_meta: Mutex::new(HashMap::new()),
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
        // 停止所有监控采样 + 清理流量统计
        self.monitor_sampler.stop_all().await;
        self.terminal_stats.lock().await.clear();
        self.sftp_clients.lock().await.clear();
        self.terminal_scrollback.lock().await.clear();
        self.session_meta.lock().await.clear();
        tracing::info!("会话清理完成");
    }
}

