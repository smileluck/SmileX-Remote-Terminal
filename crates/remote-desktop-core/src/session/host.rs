//! # Host 会话实现（连 macOS 被控服务）
//!
//! 阶段 5 实现：基于自研 Host 协议（TCP + TLS 1.3 + VP9）连接 macOS 被控服务。

use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::error::{Error, Result};
use crate::frame::DesktopFrame;
use crate::input::InputEvent;
use crate::session::{RemoteDesktopSession, SessionConfig, SessionKind};

/// Host 协议会话（连 macOS 被控服务）
pub struct HostSession {
    /// 会话 ID
    id: String,
}

impl HostSession {
    /// 创建 Host 会话实例
    pub fn new(id: String) -> Self {
        Self { id }
    }
}

#[async_trait]
impl RemoteDesktopSession for HostSession {
    fn id(&self) -> &str {
        &self.id
    }

    fn kind(&self) -> SessionKind {
        SessionKind::Host
    }

    async fn start(&mut self, _config: &SessionConfig) -> Result<mpsc::Receiver<DesktopFrame>> {
        // 阶段 5 实现：TLS 握手 + 预共享密钥认证 + VP9 帧解码循环
        Err(Error::Connect(
            "Host 协议尚未实现（阶段 5 完成，依赖 host-service 开发）".into(),
        ))
    }

    async fn send_input(&self, _event: InputEvent) -> Result<()> {
        Err(Error::Input("Host 输入未实现（阶段 5）".into()))
    }

    async fn resize(&self, _width: u32, _height: u32) -> Result<()> {
        Err(Error::Unsupported("Host resize 未实现（阶段 5）".into()))
    }

    async fn sync_clipboard(&self, _text: String) -> Result<()> {
        Err(Error::Unsupported("Host 剪贴板未实现（阶段 5）".into()))
    }

    async fn disconnect(&self) -> Result<()> {
        Ok(())
    }
}
