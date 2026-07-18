//! # remote-desktop-core 远程桌面领域核心
//!
//! 提供 Win/Linux（RDP）与 macOS（自研 Host 协议）的统一远程桌面抽象。
//!
//! ## 模块划分
//! - [`session`]：`RemoteDesktopSession` trait + 平台路由
//!   - `rdp`：RDP 实现（连 Win/Linux，阶段 3 接入 ironrdp）
//!   - `host`：自研 Host 协议客户端（连 macOS 被控服务，阶段 5 实现）
//! - [`frame`]：帧数据结构与解码
//! - [`input`]：输入事件（鼠标/键盘/滚轮）
//! - [`error`]：统一错误类型
//!
//! ## 设计要点
//! - 对前端透明：前端只接收 RGBA 帧，不感知协议差异
//! - 应用层按 `platform` 字段自动路由到 RdpSession / HostSession

pub mod error;
pub mod frame;
pub mod input;
pub mod session;

pub use error::{Error, Result};
pub use frame::DesktopFrame;
pub use input::InputEvent;
pub use session::{RemoteDesktopSession, SessionConfig, SessionKind};
