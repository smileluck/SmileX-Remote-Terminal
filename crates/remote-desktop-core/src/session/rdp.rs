//! # RDP 会话实现（连 Windows/Linux）
//!
//! MVP 阶段为占位骨架，阶段 3 接入 ironrdp 完整实现。

use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::error::{Error, Result};
use crate::frame::DesktopFrame;
use crate::input::InputEvent;
use crate::session::{RemoteDesktopSession, SessionConfig, SessionKind};

/// RDP 会话
pub struct RdpSession {
    /// 会话 ID
    id: String,
}

impl RdpSession {
    /// 创建 RDP 会话实例
    pub fn new(id: String) -> Self {
        Self { id }
    }
}

#[async_trait]
impl RemoteDesktopSession for RdpSession {
    fn id(&self) -> &str {
        &self.id
    }

    fn kind(&self) -> SessionKind {
        SessionKind::Rdp
    }

    async fn start(&mut self, _config: &SessionConfig) -> Result<mpsc::Receiver<DesktopFrame>> {
        // 阶段 3 实现：基于 ironrdp 建立 RDP 连接 + NLA 认证 + 帧解码循环
        // 当前为骨架占位，返回错误，调用方应据此提示"尚未实现"
        let (_tx, rx) = mpsc::channel::<DesktopFrame>(8);
        // 保留 rx 避免 unused 警告；实际错误由调用方处理
        let _ = rx;
        Err(Error::Connect(
            "RDP 实现尚未接入 ironrdp（阶段 3 完成）".into(),
        ))
    }

    async fn send_input(&self, _event: InputEvent) -> Result<()> {
        Err(Error::Input("RDP 未实现（阶段 3）".into()))
    }

    async fn resize(&self, _width: u32, _height: u32) -> Result<()> {
        Err(Error::Unsupported("RDP resize 未实现（阶段 3）".into()))
    }

    async fn sync_clipboard(&self, _text: String) -> Result<()> {
        Err(Error::Unsupported("RDP 剪贴板未实现（阶段 4）".into()))
    }

    async fn disconnect(&self) -> Result<()> {
        Ok(())
    }
}
