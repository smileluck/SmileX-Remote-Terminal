//! # SSH 连接管理
//!
//! 负责 russh `client::Handle` 的生命周期管理。
//! 一个 [`SshSession`] 对应一条已建立的 SSH 连接，可在其上打开多个通道（PTY/SFTP/转发）。
//!
//! ## 实现要点
//! - 基于 russh 0.45 异步 SSH 协议库（纯 Rust + Tokio）
//! - `HostKeyHandler` 实现 `client::Handler`，负责 host key 校验
//! - 认证支持：密码 / 私钥文件 / 内存 PEM 私钥
//! - PTY 打开：`channel_open_session` → `request_pty` → `request_shell`
//! - 写入：通过 `Handle::data(channel_id, ...)` 路由到指定通道
//! - 调整尺寸：通过 `Handle::window_change_request` 发送 window-change 请求

use std::collections::HashMap;
use std::net::ToSocketAddrs;
use std::sync::Arc;

use async_trait::async_trait;
use russh::client;
use russh::keys::key::PublicKey;
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
        /// 私钥文件绝对路径
        path: String,
        /// 私钥保护口令（若已加密）
        passphrase: Option<String>,
    },
    /// 私钥认证（内存 PEM 字符串 + 可选 passphrase）
    PrivateKeyMem {
        /// PEM 格式私钥文本
        key_data: String,
        /// 私钥保护口令（若已加密）
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
    /// 首次连接是否自动接受 host key
    ///
    /// - `true`：开发/调试用，跳过 known_hosts 校验，自动信任
    /// - `false`（默认，生产推荐）：拒绝未知指纹
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

/// PTY 通道句柄
///
/// 包装 russh 真实 channel。`channel` 在 [`crate::terminal::TerminalStream::start`] 中
/// 被取走驱动读循环；写入/调整尺寸通过 `SshSession` 的 `Handle` 走 channel_id 路由。
pub struct PtyHandle {
    /// russh 分配的 channel id（`ChannelId` 不可从 u32 重建，故直接保留原类型）
    pub channel_id: russh::ChannelId,
    /// russh 真实 channel（驱动读循环用）
    pub channel: russh::Channel<client::Msg>,
}

/// host key 校验处理器
///
/// 实现 `russh::client::Handler`，目前只覆盖 `check_server_key`。
/// 生产环境应替换为：弹出 UI → 用户对比指纹 → 写入 known_hosts。
struct HostKeyHandler {
    /// 是否自动接受首次 host key
    accept_first: bool,
}

#[async_trait]
impl client::Handler for HostKeyHandler {
    /// Handler 级错误类型（直接复用 russh 自带）
    type Error = russh::Error;

    /// 校验服务端 host key
    ///
    /// - `accept_first=true`：开发模式，无条件接受（避免阻断 CI/本地测试）
    /// - `accept_first=false`：默认拒绝；后续阶段接入 known_hosts + UI 确认
    ///
    /// 注：返回类型必须用 `std::result::Result` 全路径，避免与 `crate::error::Result`
    /// （单泛型别名）冲突。
    async fn check_server_key(
        &mut self,
        server_public_key: &PublicKey,
    ) -> std::result::Result<bool, Self::Error> {
        if self.accept_first {
            tracing::warn!(
                fingerprint = %server_public_key.fingerprint(),
                "host key 校验：accept_first=true，自动接受（开发模式）"
            );
            return Ok(true);
        }
        tracing::warn!(
            fingerprint = %server_public_key.fingerprint(),
            "host key 校验：accept_first=false，默认拒绝；后续阶段接入 known_hosts + UI 确认"
        );
        // MVP：默认拒绝。阶段 3 在此回调到 Tauri 弹窗确认。
        Ok(false)
    }
}

/// 单个已建立的 SSH 会话
///
/// 封装 `client::Handle`，提供通道打开、写入、尺寸调整、断开能力。
/// 线程安全，可在多个 tokio task 间共享。
pub struct SshSession {
    /// 会话唯一 ID（应用层 sessionId 关联）
    pub id: String,
    /// 目标地址（用于日志）
    pub addr: String,
    /// russh 客户端 handle（线程安全包装）
    handle: Mutex<client::Handle<HostKeyHandler>>,
    /// 是否已主动断开
    closed: Mutex<bool>,
}

impl SshSession {
    /// 建立 SSH 连接（TCP 握手 + 认证）
    ///
    /// 流程：
    /// 1. 前置解析目标地址（早期失败）
    /// 2. 构造 russh client::Config + HostKeyHandler
    /// 3. `client::connect` 建立 TCP+SSH 握手
    /// 4. 根据认证方式执行 `authenticate_*`
    /// 5. 校验认证结果
    pub async fn connect(config: &ConnectionConfig) -> Result<Self> {
        let session_id = Uuid::new_v4().to_string();

        // 1) 前置地址解析（早期失败提示）
        let addr_str = format!("{}:{}", config.host, config.port);
        let _ = addr_str
            .to_socket_addrs()
            .map_err(|e| Error::Connect(format!("地址解析失败 {addr_str}: {e}")))?
            .next()
            .ok_or_else(|| Error::Connect(format!("无法解析地址: {addr_str}")))?;

        // 2) russh client 配置（默认参数；后续阶段按需调 inactivity_timeout 等）
        let ssh_config = Arc::new(client::Config::default());
        let handler = HostKeyHandler {
            accept_first: config.accept_first_host_key,
        };

        // 3) TCP + SSH 协议握手
        tracing::info!(session_id = %session_id, addr = %addr_str, "正在建立 SSH 连接...");
        let mut handle = client::connect(ssh_config, (config.host.as_str(), config.port), handler)
            .await
            .map_err(|e| Error::Connect(format!("SSH 连接失败 {addr_str}: {e}")))?;

        // 4) 认证（按分支调用对应 API）
        let auth_ok = match &config.auth {
            AuthMethod::Password(pwd) => {
                handle
                    .authenticate_password(config.username.clone(), pwd.clone())
                    .await
                    .map_err(|e| Error::Auth(format!("密码认证请求失败: {e}")))?
            }
            AuthMethod::PrivateKey { path, passphrase } => {
                // 从文件加载私钥
                let key_pair = russh_keys::load_secret_key(path, passphrase.as_deref())
                    .map_err(|e| Error::Key(format!("加载私钥失败 {path}: {e}")))?;
                handle
                    .authenticate_publickey(&config.username, Arc::new(key_pair))
                    .await
                    .map_err(|e| Error::Auth(format!("公钥认证请求失败: {e}")))?
            }
            AuthMethod::PrivateKeyMem {
                key_data,
                passphrase,
            } => {
                // 从内存 PEM 解析私钥（适用于 OS Keyring 取出后直接认证）
                let key_pair = russh_keys::decode_secret_key(key_data, passphrase.as_deref())
                    .map_err(|e| Error::Key(format!("解析内存私钥失败: {e}")))?;
                handle
                    .authenticate_publickey(&config.username, Arc::new(key_pair))
                    .await
                    .map_err(|e| Error::Auth(format!("公钥认证请求失败: {e}")))?
            }
        };

        // 5) 校验认证结果
        if !auth_ok {
            // 主动断开已建立的 TCP 连接，避免资源泄漏
            let _ = handle
                .disconnect(russh::Disconnect::ByApplication, "", "en")
                .await;
            return Err(Error::Auth(format!(
                "认证被拒：{}@{}",
                config.username, addr_str
            )));
        }

        tracing::info!(
            session_id = %session_id,
            addr = %addr_str,
            user = %config.username,
            "SSH 会话已建立"
        );

        Ok(Self {
            id: session_id,
            addr: addr_str,
            handle: Mutex::new(handle),
            closed: Mutex::new(false),
        })
    }

    /// 打开一个 PTY 终端通道
    ///
    /// 流程：`channel_open_session` → `request_pty` → `request_shell`
    /// 返回的 [`PtyHandle`] 持有真实 channel，由 terminal 模块驱动读循环。
    pub async fn open_pty(&self, cols: u32, rows: u32) -> Result<PtyHandle> {
        let handle = self.handle.lock().await;

        // 打开 session 类型通道
        let channel = handle
            .channel_open_session()
            .await
            .map_err(|e| Error::Terminal(format!("打开 session channel 失败: {e}")))?;

        let channel_id = channel.id();

        // 请求 PTY（终端类型 xterm-256color，无特殊 terminal_modes）
        channel
            .request_pty(
                false,             // want_reply
                "xterm-256color",  // TERM 环境变量值
                cols,              // 列数
                rows,              // 行数
                0,                 // 像素宽度（0 表示未知）
                0,                 // 像素高度（0 表示未知）
                &[],               // terminal_modes（终端特性，空数组表示默认）
            )
            .await
            .map_err(|e| Error::Terminal(format!("request_pty 失败: {e}")))?;

        // 请求 shell（启动交互式会话）
        channel
            .request_shell(false)
            .await
            .map_err(|e| Error::Terminal(format!("request_shell 失败: {e}")))?;

        tracing::debug!(
            session_id = %self.id,
            channel_id = %channel_id,
            cols,
            rows,
            "PTY 通道已建立"
        );
        Ok(PtyHandle {
            channel_id,
            channel,
        })
    }

    /// 向指定 channel 写入数据（终端输入）
    ///
    /// 通过 `Handle::data(channel_id, ...)` 路由，
    /// 即使 channel 对象已被 [`TerminalStream`] 持有，Handle 仍可按 id 写入。
    pub async fn write(&self, channel_id: russh::ChannelId, data: &[u8]) -> Result<()> {
        let handle = self.handle.lock().await;
        // CryptoVec 仅支持 From<Vec<u8>> / From<String>，需主动 to_vec
        let crypto_data = russh::CryptoVec::from(data.to_vec());
        handle
            .data(channel_id, crypto_data)
            .await
            .map_err(|unsent| {
                Error::Terminal(format!(
                    "写入 channel {channel_id} 失败：{:?} 字节未送达",
                    unsent.len()
                ))
            })?;
        Ok(())
    }
    /// 调整终端窗口尺寸（发送 window-change 请求）
    ///
    /// **阶段 2 限制**：russh 0.45 的 `Handle` 不提供 window-change 路由方法，
    /// `Channel::window_change(&self)` 又与 `Channel::wait(&mut self)` 互斥。
    /// 阶段 3 计划：将 TerminalStream 改造为 `select!` 循环 + 独立控制 mpsc，
    /// 在读循环内部调用 `channel.window_change()`，此处通过控制通道投递指令。
    ///
    /// 当前实现：直接返回 Ok，记录 TODO 日志。
    pub async fn resize(&self, channel_id: russh::ChannelId, cols: u32, rows: u32) -> Result<()> {
        tracing::warn!(
            channel_id = %channel_id,
            cols, rows,
            "window_change 暂未实现（阶段 3 通过 select! + 控制通道接入）"
        );
        // TODO(阶段 3): 通过 TerminalStream 控制通道投递 WindowChange 指令
        let _ = channel_id;
        Ok(())
    }

    /// 主动断开会话（幂等：多次调用安全）
    ///
    /// 发送 SSH `DISCONNECT` 消息（ByApplication），通知对端优雅关闭。
    pub async fn disconnect(&self) -> Result<()> {
        let mut closed = self.closed.lock().await;
        if !*closed {
            *closed = true;
            let handle = self.handle.lock().await;
            let _ = handle
                .disconnect(russh::Disconnect::ByApplication, "", "en")
                .await;
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

    /// 建立新会话并注册（失败时不污染注册表）
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
