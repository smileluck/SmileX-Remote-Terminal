//! # 统一错误类型
//!
//! 为 ssh-core 提供统一的错误枚举，避免把底层库错误类型泄漏到应用层。

use thiserror::Error;

/// ssh-core 统一错误类型
#[derive(Debug, Error)]
pub enum Error {
    /// SSH 连接失败（网络/认证/协议）
    #[error("SSH 连接失败: {0}")]
    Connect(String),

    /// SSH 认证失败（密码/密钥错误）
    #[error("SSH 认证失败: {0}")]
    Auth(String),

    /// known_hosts 校验失败（指纹不匹配，疑似中间人）
    #[error("known_hosts 校验失败: {0}")]
    HostKey(String),

    /// 终端通道错误
    #[error("终端通道错误: {0}")]
    Terminal(String),

    /// 密钥解析/生成错误
    #[error("密钥错误: {0}")]
    Key(String),

    /// 端口转发隧道错误（监听绑定 / direct-tcpip / tcpip-forward）
    #[error("隧道错误: {0}")]
    Tunnel(String),

    /// IO 错误
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),

    /// 操作被取消（用户取消 / 暂停 / 会话断开）
    #[error("操作已取消")]
    Canceled,

    /// 其他底层错误
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// 统一 Result 别名
pub type Result<T> = std::result::Result<T, Error>;
