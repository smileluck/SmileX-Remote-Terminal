//! # 远程桌面会话抽象
//!
//! `RemoteDesktopSession` trait 统一 RDP（Win/Linux）与 Host 协议（macOS）。
//! 应用层按 [`SessionConfig::kind`] 路由到具体实现。

pub mod host;
pub mod rdp;

use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::error::Result;
use crate::frame::DesktopFrame;
use crate::input::InputEvent;

/// 会话协议种类
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SessionKind {
    /// RDP（连 Windows/Linux）
    Rdp,
    /// 自研 Host 协议（连 macOS 被控服务）
    Host,
}

/// 远程桌面连接配置
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SessionConfig {
    /// 会话种类（决定路由到哪个实现）
    pub kind: SessionKind,
    /// 目标主机
    pub host: String,
    /// 目标端口（RDP 默认 3389，Host 默认自定义端口）
    pub port: u16,
    /// 用户名
    pub username: String,
    /// 密码/预共享密钥（应用层从 Keyring 取后传入）
    pub password: String,
    /// 初始分辨率宽
    pub width: u32,
    /// 初始分辨率高
    pub height: u32,
    /// 色深（8/16/24/32）
    pub color_depth: u8,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            kind: SessionKind::Rdp,
            host: "127.0.0.1".into(),
            port: 3389,
            username: String::new(),
            password: String::new(),
            width: 1920,
            height: 1080,
            color_depth: 32,
        }
    }
}

/// 远程桌面会话 trait —— 所有协议实现此接口
#[async_trait]
pub trait RemoteDesktopSession: Send {
    /// 会话 ID
    fn id(&self) -> &str;

    /// 协议种类
    fn kind(&self) -> SessionKind;

    /// 启动会话：建立连接，返回帧接收端
    ///
    /// 实现内部拉起读循环，将解码后的帧推送到 mpsc。
    async fn start(&mut self, config: &SessionConfig) -> Result<mpsc::Receiver<DesktopFrame>>;

    /// 发送输入事件
    async fn send_input(&self, event: InputEvent) -> Result<()>;

    /// 调整分辨率/画质（自适应带宽）
    async fn resize(&self, width: u32, height: u32) -> Result<()>;

    /// 同步剪贴板
    async fn sync_clipboard(&self, text: String) -> Result<()>;

    /// 主动断开
    async fn disconnect(&self) -> Result<()>;
}

/// 按 [`SessionConfig::kind`] 路由创建会话
pub fn create_session(session_id: String, kind: SessionKind) -> Box<dyn RemoteDesktopSession> {
    match kind {
        SessionKind::Rdp => Box::new(rdp::RdpSession::new(session_id)),
        SessionKind::Host => Box::new(host::HostSession::new(session_id)),
    }
}
