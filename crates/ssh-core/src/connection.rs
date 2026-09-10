//! # SSH 连接管理
//!
//! 负责 russh `client::Handle` 的生命周期管理。
//! 一个 [`SshSession`] 对应一条已建立的 SSH 连接，可在其上打开多个通道（PTY/SFTP/转发）。
//!
//! ## 实现要点
//! - 基于 russh 0.45 异步 SSH 协议库（纯 Rust + Tokio）
//! - `HostKeyHandler` 实现 `client::Handler`，通过 [`KnownHostsStore`] 校验 host key
//! - 认证支持：密码 / 私钥文件 / 内存 PEM 私钥
//! - PTY 打开：`channel_open_session` → `request_pty` → `request_shell`
//! - 写入：通过 `Handle::data(channel_id, ...)` 路由到指定通道
//! - 调整尺寸：通过 [`crate::terminal::TerminalControl`] 投递到 select! 读循环

use std::collections::HashMap;
use std::future::Future;
use std::net::ToSocketAddrs;
use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use russh::client;
use russh::keys::key::PublicKey;
use tokio::sync::{mpsc, Mutex};
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::known_hosts::{
    HostKeyPolicy, KnownHost, KnownHostsStore, verify_host_key,
};

/// SSH 认证方式
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
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
#[serde(rename_all = "camelCase")]
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

/// host key 确认挑战信息（未知主机时交由应用层 UI 确认）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HostKeyChallenge {
    pub host: String,
    pub port: u16,
    pub key_type: String,
    pub fingerprint: String,
}

/// Remote 转发（tcpip-forward）的入站连接
///
/// 服务器接受一条到远端监听端口的 TCP 连接后，通过
/// `forwarded-tcpip` 通道回调（`server_channel_open_forwarded_tcpip`）
/// 交给客户端；tunnel 模块据此把通道桥接到本地目标。
pub struct ForwardedConn {
    /// russh 真实通道（`into_stream` 后与本地 TCP 连接双向拷贝）
    pub channel: russh::Channel<client::Msg>,
    /// 连接到达的远端监听端口（用于按端口分发给对应隧道）
    pub connected_port: u32,
    /// 连接来源地址（日志用）
    pub originator_address: String,
    /// 连接来源端口（日志用）
    pub originator_port: u32,
}

/// host key 交互式确认回调的返回 Future
pub type ConfirmFuture = Pin<Box<dyn Future<Output = bool> + Send>>;

/// 未知主机 host key 的交互式确认回调
///
/// 应用层（desktop）注入：emit 事件给前端弹窗，等待用户选择。
/// 返回 `true` 表示信任并保存；超时/拒绝返回 `false`。
pub type HostKeyConfirm = Arc<dyn Fn(HostKeyChallenge) -> ConfirmFuture + Send + Sync>;

/// host key 校验处理器
///
/// 实现 `russh::client::Handler`，目前只覆盖 `check_server_key`。
///
/// 校验流程：
/// 1. 从 store 查询 `host:port` 的已知记录
/// 2. 调用 [`verify_host_key`] 按策略判断
/// 3. 若策略为 `AcceptNew` 且接受 → 保存到 store（后续按已知处理）
/// 4. 未知主机被拒时，若注入了 [`HostKeyConfirm`] 回调 → 交给用户确认
struct HostKeyHandler {
    /// 主机（用于 store 查询/保存）
    host: String,
    /// 端口（用于 store 查询/保存）
    port: u16,
    /// 校验策略
    policy: HostKeyPolicy,
    /// 已知主机存储（应用层注入）
    store: Arc<dyn KnownHostsStore>,
    /// 未知主机交互式确认回调（应用层注入，可选）
    confirm: Option<HostKeyConfirm>,
    /// Remote 转发入站连接投递口（connect 时创建，receiver 存于 SshSession）
    forwarded_tx: mpsc::Sender<ForwardedConn>,
}

#[async_trait]
impl client::Handler for HostKeyHandler {
    /// Handler 级错误类型（直接复用 russh 自带）
    type Error = russh::Error;

    /// 校验服务端 host key
    ///
    /// 1. 提取 `key_type` + `fingerprint`
    /// 2. 查询 store（出错按 None 处理，避免单次 IO 失败阻断）
    /// 3. 按策略判断接受/拒绝
    /// 4. `AcceptNew` 首次接受 → 落盘保存
    ///
    /// 注：返回类型必须用 `std::result::Result` 全路径，避免与 `crate::error::Result`
    /// （单泛型别名）冲突。
    async fn check_server_key(
        &mut self,
        server_public_key: &PublicKey,
    ) -> std::result::Result<bool, Self::Error> {
        let key_type = server_public_key.name().to_string();
        let fingerprint = server_public_key.fingerprint().to_string();

        // 查询 store（错误降级为 None + 警告日志）
        let known = match self.store.lookup(&self.host, self.port).await {
            Ok(Some(entry)) => Some(entry),
            Ok(None) => None,
            Err(e) => {
                tracing::warn!(
                    host = %self.host,
                    port = self.port,
                    error = %e,
                    "查询 known_hosts 失败，按未知主机处理"
                );
                None
            }
        };

        let mut accepted = verify_host_key(&key_type, &fingerprint, known.as_ref(), self.policy);

        // 未知主机被策略拒绝时，交由应用层交互式确认（UI 弹窗）
        if !accepted && known.is_none() {
            if let Some(confirm) = &self.confirm {
                let challenge = HostKeyChallenge {
                    host: self.host.clone(),
                    port: self.port,
                    key_type: key_type.clone(),
                    fingerprint: fingerprint.clone(),
                };
                accepted = confirm(challenge).await;
                if accepted {
                    // 用户明确信任 → 落盘保存
                    let entry = KnownHost {
                        host: self.host.clone(),
                        port: self.port,
                        key_type: key_type.clone(),
                        fingerprint: fingerprint.clone(),
                    };
                    if let Err(e) = self.store.save(entry).await {
                        tracing::warn!(
                            host = %self.host,
                            port = self.port,
                            error = %e,
                            "保存 known_hosts 失败（本次仍接受，下次可能再次提示）"
                        );
                    }
                }
            }
        }

        if accepted {
            // AcceptNew 策略下若为首次（known=None），写入 store
            if matches!(self.policy, HostKeyPolicy::AcceptNew) && known.is_none() {
                let entry = KnownHost {
                    host: self.host.clone(),
                    port: self.port,
                    key_type: key_type.clone(),
                    fingerprint: fingerprint.clone(),
                };
                if let Err(e) = self.store.save(entry).await {
                    tracing::warn!(
                        host = %self.host,
                        port = self.port,
                        error = %e,
                        "保存 known_hosts 失败（本次仍接受，下次可能再次提示）"
                    );
                }
            }
            tracing::info!(
                host = %self.host,
                port = self.port,
                key_type = %key_type,
                fingerprint = %fingerprint,
                policy = ?self.policy,
                "host key 校验通过"
            );
        } else {
            tracing::warn!(
                host = %self.host,
                port = self.port,
                key_type = %key_type,
                fingerprint = %fingerprint,
                policy = ?self.policy,
                "host key 校验拒绝（指纹不匹配或策略要求已知主机）"
            );
        }

        Ok(accepted)
    }

    /// Remote 转发（tcpip-forward）的入站通道回调
    ///
    /// russh 默认实现为 no-op（直接丢弃通道）。这里把通道与来源信息
    /// 投递到 forwarded channel，由 tunnel 模块消费并桥接到本地目标。
    /// 投递失败（无接收方：未启用 remote 隧道）时仅记日志，通道随作用域关闭。
    async fn server_channel_open_forwarded_tcpip(
        &mut self,
        channel: russh::Channel<client::Msg>,
        _connected_address: &str,
        connected_port: u32,
        originator_address: &str,
        originator_port: u32,
        _session: &mut client::Session,
    ) -> std::result::Result<(), Self::Error> {
        let conn = ForwardedConn {
            channel,
            connected_port,
            originator_address: originator_address.to_string(),
            originator_port,
        };
        if let Err(e) = self.forwarded_tx.send(conn).await {
            tracing::warn!(
                connected_port,
                error = %e,
                "forwarded-tcpip 入站通道无接收方（remote 隧道未就绪），已丢弃"
            );
        }
        Ok(())
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
    /// Remote 转发入站连接接收端（connect 时创建；首条 remote 隧道取走后
    /// 由 TunnelManager 的按端口分发器持有，故只能 take 一次）
    forwarded_rx: Mutex<Option<mpsc::Receiver<ForwardedConn>>>,
    /// 连接配置与已知主机存储的副本（SFTP 大流量轮换重连用）
    pub(crate) reconnect: (
        ConnectionConfig,
        Arc<dyn KnownHostsStore>,
    ),
}

impl SshSession {
    /// 建立 SSH 连接（TCP 握手 + 认证 + host key 校验）
    ///
    /// 流程：
    /// 1. 前置解析目标地址（早期失败）
    /// 2. 构造 russh client::Config + HostKeyHandler（注入 store + policy）
    /// 3. `client::connect` 建立 TCP+SSH 握手
    /// 4. 根据认证方式执行 `authenticate_*`
    /// 5. 校验认证结果
    ///
    /// # 参数
    /// - `config`：连接配置（地址 / 认证 / `accept_first_host_key` 标志）
    /// - `known_hosts`：已知主机存储（由应用层注入，如 SQLite）
    /// - `confirm`：未知主机交互式确认回调（UI 弹窗，可选）
    pub async fn connect(
        config: &ConnectionConfig,
        known_hosts: Arc<dyn KnownHostsStore>,
        confirm: Option<HostKeyConfirm>,
    ) -> Result<Self> {
        let session_id = Uuid::new_v4().to_string();

        // 1) 前置地址解析（早期失败提示）
        let addr_str = format!("{}:{}", config.host, config.port);
        let _ = addr_str
            .to_socket_addrs()
            .map_err(|e| Error::Connect(format!("地址解析失败 {addr_str}: {e}")))?
            .next()
            .ok_or_else(|| Error::Connect(format!("无法解析地址: {addr_str}")))?;

        // 2) russh client 配置 + HostKeyHandler（注入 store）
        //    forwarded channel：remote 转发入站连接回调 → 隧道分发（容量 64 防突发堆积）
        let ssh_config = Arc::new(client::Config::default());
        let (forwarded_tx, forwarded_rx) = mpsc::channel::<ForwardedConn>(64);
        let handler = HostKeyHandler {
            host: config.host.clone(),
            port: config.port,
            policy: HostKeyPolicy::from_accept_first(config.accept_first_host_key),
            store: known_hosts.clone(),
            confirm,
            forwarded_tx,
        };

        // 3) TCP + SSH 协议握手
        tracing::info!(session_id = %session_id, addr = %addr_str, "正在建立 SSH 连接...");
        let mut handle = client::connect(ssh_config, (config.host.as_str(), config.port), handler)
            .await
            .map_err(|e| Error::Connect(format!("SSH 连接失败 {addr_str}: {e}")))?;

        // 4) 认证（按分支调用对应 API）
        let auth_method_name = match &config.auth {
            AuthMethod::Password(_) => "密码认证",
            AuthMethod::PrivateKey { .. } => "私钥文件认证",
            AuthMethod::PrivateKeyMem { .. } => "密钥管理器认证",
        };
        let auth_ok = match &config.auth {
            AuthMethod::Password(pwd) => {
                if pwd.is_empty() {
                    return Err(Error::Auth(format!(
                        "密码为空：Keyring 中未取到该会话的密码，请编辑会话重新保存（{}@{}）",
                        config.username, addr_str
                    )));
                }
                let ok = handle
                    .authenticate_password(config.username.clone(), pwd.clone())
                    .await
                    .map_err(|e| Error::Auth(format!("密码认证请求失败: {e}")))?;
                if ok {
                    true
                } else {
                    // 密码方法被拒 → keyboard-interactive 兜底（部分服务器只开启该方式，如 NAS/PAM 配置）
                    tracing::info!(session_id = %session_id, "密码认证被拒，尝试 keyboard-interactive 兜底");
                    keyboard_interactive_fallback(&mut handle, &config.username, pwd)
                        .await
                        .map_err(|e| Error::Auth(format!("keyboard-interactive 认证请求失败: {e}")))?
                }
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
                "认证被拒（{}@{}，{}）：密码/密钥错误，或服务器未开启该认证方式",
                config.username, addr_str, auth_method_name
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
            forwarded_rx: Mutex::new(Some(forwarded_rx)),
            reconnect: (config.clone(), known_hosts.clone()),
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
    /// 在独立通道上执行一次性命令并收集输出（非交互 exec，用于监控采集等）
    ///
    /// 流程：`channel_open_session` → `channel.exec` → 循环 `wait()` 收集
    /// stdout/stderr 直到 EOF/Close。超时由调用方（tokio::time::timeout）控制。
    pub async fn exec(&self, command: &str) -> Result<String> {
        let mut channel = {
            let handle = self.handle.lock().await;
            handle
                .channel_open_session()
                .await
                .map_err(|e| Error::Terminal(format!("打开 exec channel 失败: {e}")))?
        };

        channel
            .exec(true, command)
            .await
            .map_err(|e| Error::Terminal(format!("exec 请求失败: {e}")))?;

        let mut out: Vec<u8> = Vec::new();
        loop {
            match channel.wait().await {
                Some(russh::ChannelMsg::Data { ref data }) => {
                    out.extend_from_slice(data)
                }
                Some(russh::ChannelMsg::ExtendedData { ref data, .. }) => {
                    out.extend_from_slice(data)
                }
                Some(russh::ChannelMsg::ExitStatus { exit_status }) => {
                    if exit_status != 0 {
                        tracing::warn!(
                            session_id = %self.id,
                            exit_status,
                            "exec 命令返回非零退出码"
                        );
                    }
                }
                // EOF / Close / None：通道结束
                Some(russh::ChannelMsg::Eof)
                | Some(russh::ChannelMsg::Close)
                | None => break,
                _ => {}
            }
        }
        Ok(String::from_utf8_lossy(&out).into_owned())
    }

    /// 在当前连接上打开 SFTP 通道并初始化协议
    ///
    /// 每次调用建立独立通道；调用方（应用层）可按 sessionId 缓存复用。
    /// 在当前连接上打开 SFTP 通道并初始化协议
    ///
    /// 每次调用建立独立通道；调用方（应用层）可按 sessionId 缓存复用。
    /// 返回的 [`SftpClient`] 持有本会话弱引用与重连上下文，用于累计
    /// 流量达到阈值后透明轮换到新 SSH 连接（规避 russh 0.45 会话循环
    /// 在单连接 ~1GiB 传输量后退出的问题）。
    pub async fn open_sftp(self: &Arc<Self>) -> Result<crate::sftp::SftpClient> {
        let session = self.open_sftp_channel().await?;
        Ok(crate::sftp::SftpClient::new(session, Arc::downgrade(self)))
    }

    /// 打开一条新的 SFTP 通道（通道开 + 子系统请求 + 协议初始化）
    pub async fn open_sftp_channel(&self) -> Result<russh_sftp::client::SftpSession> {
        let channel = {
            let handle = self.handle.lock().await;
            handle
                .channel_open_session()
                .await
                .map_err(|e| Error::Terminal(format!("打开 SFTP channel 失败: {e}")))?
        };
        // SftpSession 只负责在流上收发 SFTP 包，子系统请求必须由此处显式发出，
        // 否则对端不会启动 sftp-server，初始化将一直等到超时
        channel
            .request_subsystem(true, "sftp")
            .await
            .map_err(|e| Error::Terminal(format!("请求 SFTP 子系统失败: {e}")))?;

        russh_sftp::client::SftpSession::new(channel.into_stream())
            .await
            .map_err(|e| Error::Terminal(format!("SFTP 协议初始化失败: {e}")))
    }

    /// 打开 direct-tcpip 通道（Local / Dynamic 转发的数据面出口）
    ///
    /// 让 SSH 服务器代连 `host:port`，返回的通道经 `into_stream()`
    /// 与本地 TCP 连接双向桥接。originator 填回环地址（仅协议字段）。
    pub async fn open_direct_tcpip(
        &self,
        host: &str,
        port: u32,
    ) -> Result<russh::Channel<client::Msg>> {
        let handle = self.handle.lock().await;
        handle
            .channel_open_direct_tcpip(host, port, "127.0.0.1", 0)
            .await
            .map_err(|e| Error::Tunnel(format!("打开 direct-tcpip 通道失败 {host}:{port}: {e}")))
    }

    /// 请求服务器在 `address:port` 上监听并把连接转发回本端（Remote 转发）
    ///
    /// `port = 0` 表示由服务器分配端口，返回实际监听的端口号。
    /// 入站连接经 [`SshSession::take_forwarded_rx`] 取出后消费。
    pub async fn tcpip_forward(&self, address: &str, port: u32) -> Result<u32> {
        let mut handle = self.handle.lock().await;
        handle
            .tcpip_forward(address, port)
            .await
            .map_err(|e| Error::Tunnel(format!("请求远端转发失败 {address}:{port}: {e}")))
    }

    /// 取消 Remote 转发监听（隧道停止时调用；失败仅记录，不阻断清理）
    pub async fn cancel_tcpip_forward(&self, address: &str, port: u32) -> Result<()> {
        let handle = self.handle.lock().await;
        handle
            .cancel_tcpip_forward(address, port)
            .await
            .map_err(|e| Error::Tunnel(format!("取消远端转发失败 {address}:{port}: {e}")))
    }

    /// 取走 Remote 转发入站连接接收端（仅首次调用成功）
    ///
    /// 全局只有一个接收端，由 TunnelManager 的按端口分发器持有，
    /// 各 remote 隧道向分发器注册自己的监听端口获取连接。
    pub async fn take_forwarded_rx(&self) -> Option<mpsc::Receiver<ForwardedConn>> {
        self.forwarded_rx.lock().await.take()
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

/// keyboard-interactive 兜底认证：用密码应答服务器全部 prompt（最多 3 轮）
///
/// 适用场景：服务器未开启 `password` 认证、仅允许 `keyboard-interactive`
/// （常见于 NAS / PAM 配置）。OpenSSH CLI 也会两种方式依次尝试。
/// 服务器不支持该方式时直接返回 `AuthFailure`，此处转为 `Ok(false)`。
async fn keyboard_interactive_fallback(
    handle: &mut client::Handle<HostKeyHandler>,
    username: &str,
    password: &str,
) -> Result<bool> {
    let mut resp = handle
        .authenticate_keyboard_interactive_start(username, None::<String>)
        .await
        .map_err(|e| Error::Auth(format!("keyboard-interactive 请求失败: {e}")))?;
    for _ in 0..3 {
        match resp {
            client::KeyboardInteractiveAuthResponse::Success => return Ok(true),
            client::KeyboardInteractiveAuthResponse::Failure => return Ok(false),
            client::KeyboardInteractiveAuthResponse::InfoRequest { prompts, .. } => {
                resp = handle
                    .authenticate_keyboard_interactive_respond(
                        prompts.into_iter().map(|_| password.to_string()).collect(),
                    )
                    .await
                    .map_err(|e| Error::Auth(format!("keyboard-interactive 应答失败: {e}")))?;
            }
        }
    }
    tracing::warn!("keyboard-interactive 认证轮次超限（3 轮），视为失败");
    Ok(false)
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
    ///
    /// # 参数
    /// - `config`：连接配置
    /// - `known_hosts`：已知主机存储（传递给 `SshSession::connect`）
    /// - `confirm`：未知主机交互式确认回调（可选，传递给 `SshSession::connect`）
    pub async fn connect(
        &self,
        config: &ConnectionConfig,
        known_hosts: Arc<dyn KnownHostsStore>,
        confirm: Option<HostKeyConfirm>,
    ) -> Result<Arc<SshSession>> {
        let session = Arc::new(SshSession::connect(config, known_hosts, confirm).await?);
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

#[cfg(test)]
mod tests {
    use super::*;

    /// 前端（app/src/types/session.ts、useSshConnect.ts）实际发送的 JSON 形状，
    /// 锁定 serde tag/字段命名，防止再次出现大小写不匹配。
    #[test]
    fn deserializes_frontend_auth_shapes() {
        let password: AuthMethod =
            serde_json::from_str(r#"{"type":"password","value":"pw"}"#).unwrap();
        assert!(matches!(password, AuthMethod::Password(p) if p == "pw"));

        let key: AuthMethod = serde_json::from_str(
            r#"{"type":"private_key","value":{"path":"/id_ed25519","passphrase":"pp"}}"#,
        )
        .unwrap();
        assert!(matches!(key, AuthMethod::PrivateKey { ref path, ref passphrase }
            if path == "/id_ed25519" && passphrase.as_deref() == Some("pp")));

        let key_mem: AuthMethod = serde_json::from_str(
            r#"{"type":"private_key_mem","value":{"key_data":"PEM"}}"#,
        )
        .unwrap();
        assert!(matches!(key_mem, AuthMethod::PrivateKeyMem { ref key_data, passphrase }
            if key_data == "PEM" && passphrase.is_none()));
    }

    /// acceptFirstHostKey 为 camelCase（Tauri 只转换顶层参数名，不递归转换嵌套字段）
    #[test]
    fn deserializes_camel_case_config_fields() {
        let cfg: ConnectionConfig = serde_json::from_str(
            r#"{"host":"h","port":22,"username":"u","auth":{"type":"password","value":"pw"},"acceptFirstHostKey":true}"#,
        )
        .unwrap();
        assert!(cfg.accept_first_host_key);
    }
}
