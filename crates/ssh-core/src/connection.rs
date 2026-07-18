//! # SSH 连接管理
//!
//! 负责 russh `client::Handle` 的生命周期管理。
//! 一个 [`SshSession`] 对应一条已建立的 SSH 连接，可在其上打开多个通道（PTY/SFTP/转发）。
//!
//! **注**：当前为阶段 1 骨架，实际 russh 连接逻辑在阶段 2 基于 russh 0.45 真实 API 补全。

use std::collections::HashMap;
use std::net::ToSocketAddrs;
use std::sync::Arc;

use tokio::sync::Mutex;
use uuid::Uuid;

use crate::error::{Error, Result};

/// SSH 认证方式
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum AuthMethod {
    /// 密码认证（密码从应用层 OS Keyring 取后传入）
    Password(String),
    /// 私钥认证（私钥路径 + 可选 passphrase）
    PrivateKey {
        path: String,
        passphrase: Option<String>,
    },
    /// 私钥认证（内存 PEM 字符串 + 可选 passphrase）
    PrivateKeyMem {
        key_data: String,
        passphrase: Option<String>,
    },
}

/// SSH 连接配置（从前端会话配置传入）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ConnectionConfig {
    /// 目标主机（IP 或域名）
    pub host: String,
    /// 目标端口（默认 22）
    pub port: u16,
    /// 登录用户名
    pub username: String,
    /// 认证方式
    pub auth: AuthMethod,
    /// 首次连接是否自动接受 host key（生产环境应改为弹窗确认）
    #[serde(default)]
    pub accept_first_host_key: bool,
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".into(),
            port: 22,
            username: "root".into(),
            auth: AuthMethod::Password(String::new()),
            accept_first_host_key: false,
        }
    }
}

/// PTY 通道句柄（阶段 2 基于真实 russh channel 实现）
///
/// 当前为骨架占位：仅保留 channel_id 用于应用层索引。
pub struct PtyHandle {
    /// russh channel id（阶段 2 接入真实类型）
    pub channel_id: u32,
}

/// host key 校验处理器配置（阶段 2 实现完整 russh Handler）
#[derive(Clone, Default)]
struct HostKeyConfig {
    accept_first: bool,
}

/// 单个已建立的 SSH 会话
///
/// 封装连接句柄，提供通道打开、关闭、断开能力。
/// 线程安全，可在多个 tokio task 间共享。
pub struct SshSession {
    /// 会话唯一 ID（应用层 sessionId 关联）
    pub id: String,
    /// 目标地址（用于日志）
    pub addr: String,
    /// 连接配置（保留用于阶段 2 真实实现）
    #[allow(dead_code)]
    config: ConnectionConfig,
    /// host key 配置
    #[allow(dead_code)]
    host_key: HostKeyConfig,
    /// 是否已断开
    closed: Mutex<bool>,
}

impl SshSession {
    /// 建立连接
    ///
    /// 阶段 1：仅校验地址解析 + 生成 sessionId，实际 SSH 握手在阶段 2 基于 russh 实现。
    pub async fn connect(config: &ConnectionConfig) -> Result<Self> {
        let session_id = Uuid::new_v4().to_string();

        // 解析目标地址（前置校验）
        let addr_str = format!("{}:{}", config.host, config.port);
        let _addr = addr_str
            .to_socket_addrs()
            .map_err(|e| Error::Connect(format!("地址解析失败 {addr_str}: {e}")))?
            .next()
            .ok_or_else(|| Error::Connect(format!("无法解析地址: {addr_str}")))?;

        tracing::info!(session_id = %session_id, addr = %addr_str, "SSH 会话占位建立（阶段 2 接入真实 russh）");

        Ok(Self {
            id: session_id,
            addr: addr_str,
            config: config.clone(),
            host_key: HostKeyConfig {
                accept_first: config.accept_first_host_key,
            },
            closed: Mutex::new(false),
        })
    }

    /// 打开一个 PTY 终端通道
    ///
    /// 阶段 1：返回占位 PtyHandle；阶段 2 接入真实 russh channel。
    pub async fn open_pty(&self, _cols: u32, _rows: u32) -> Result<PtyHandle> {
        // TODO(阶段 2): 基于 russh 0.45 真实 API 打开 channel + 请求 PTY + 请求 shell
        Ok(PtyHandle { channel_id: 0 })
    }

    /// 向会话写入数据（终端输入）
    ///
    /// 阶段 1：noop 占位。
    pub async fn write(&self, _channel_id: u32, _data: &[u8]) -> Result<()> {
        // TODO(阶段 2): 通过 russh handle.data() 发送
        Ok(())
    }

    /// 主动断开会话
    pub async fn disconnect(&self) -> Result<()> {
        let mut closed = self.closed.lock().await;
        if !*closed {
            *closed = true;
            tracing::info!(session_id = %self.id, "SSH 会话已断开");
        }
        Ok(())
    }

    /// 会话是否已断开
    pub async fn is_closed(&self) -> bool {
        *self.closed.lock().await
    }
}

impl Drop for SshSession {
    fn drop(&mut self) {
        tracing::debug!(session_id = %self.id, "SshSession dropped");
    }
}

/// 会话管理器
///
/// 应用层（desktop）通过此管理器按 sessionId 索引会话。
/// 线程安全，可从 Tauri AppState 全局共享。
#[derive(Default)]
pub struct SessionManager {
    sessions: Mutex<HashMap<String, Arc<SshSession>>>,
}

impl SessionManager {
    /// 创建空管理器
    pub fn new() -> Self {
        Self::default()
    }

    /// 建立新会话并注册
    pub async fn connect(&self, config: &ConnectionConfig) -> Result<Arc<SshSession>> {
        let session = Arc::new(SshSession::connect(config).await?);
        self.sessions
            .lock()
            .await
            .insert(session.id.clone(), session.clone());
        Ok(session)
    }

    /// 按 ID 获取会话
    pub async fn get(&self, session_id: &str) -> Option<Arc<SshSession>> {
        self.sessions.lock().await.get(session_id).cloned()
    }

    /// 按 ID 断开并移除会话
    pub async fn disconnect(&self, session_id: &str) -> Result<()> {
        if let Some(session) = self.sessions.lock().await.remove(session_id) {
            session.disconnect().await?;
        }
        Ok(())
    }

    /// 断开所有会话（应用退出时调用）
    pub async fn disconnect_all(&self) {
        let mut sessions = self.sessions.lock().await;
        for (_, session) in sessions.drain() {
            let _ = session.disconnect().await;
        }
    }
}

/// russh client::Handler 的占位（阶段 2 接入真实实现）
#[allow(unused)]
async fn _russh_handler_stub() {
    // 阶段 2 补全：
    // struct HostKeyHandler { accept_first: bool }
    // #[async_trait]
    // impl client::Handler for HostKeyHandler {
    //     async fn check_server_key(&mut self, key: &PublicKey) -> Result<bool, russh::Error> { ... }
    // }
}
