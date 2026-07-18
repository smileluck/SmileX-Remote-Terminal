//! # 统一错误类型

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    /// 远程桌面连接失败
    #[error("远程桌面连接失败: {0}")]
    Connect(String),

    /// 认证失败
    #[error("认证失败: {0}")]
    Auth(String),

    /// 帧解码错误
    #[error("帧解码错误: {0}")]
    Decode(String),

    /// 输入事件错误
    #[error("输入事件错误: {0}")]
    Input(String),

    /// 不支持的协议/平台
    #[error("不支持: {0}")]
    Unsupported(String),

    /// IO 错误
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),

    /// 其他
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
