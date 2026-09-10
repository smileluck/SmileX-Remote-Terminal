//! # 端口转发引擎（Local / Remote / Dynamic）
//!
//! 在一条已建立的 SSH 会话上承载三种端口转发：
//! - **Local**：本地监听 → SSH 服务器代连远端目标（`direct-tcpip`）
//! - **Remote**：请求服务器远端监听（`tcpip-forward`），入站连接桥接到本地目标
//! - **Dynamic**：本地 SOCKS5 代理（无认证），握手后按请求目标走 `direct-tcpip`
//!
//! ## 数据面
//! 每条入站连接独立 spawn 一个桥接 task，SSH 通道经 `into_stream()` 与
//! TCP 连接 `copy_bidirectional` 双向拷贝。隧道停止时 accept/接收循环退出，
//! JoinSet drop 连带 abort 所有连接桥接 task。
//!
//! ## Remote 转发的地址语义（与 OpenSSH `-R` 一致）
//! `TunnelConfig` 的字段在 remote 模式下是「远端视角」：
//! - `remote_host:remote_port`：远端（SSH 服务器侧）监听地址
//! - `local_host:local_port`：入站连接要跳转的本地目标（从客户端视角出发 connect）
//!
//! ## 多条 remote 隧道的分发
//! russh 的 `forwarded-tcpip` 回调只有一个接收端（[`SshSession::take_forwarded_rx`]）。
//! TunnelManager 为每个会话维护一张「监听端口 → 隧道入口」路由表，由首条
//! remote 隧道拉起的分发 task 消费接收端并按 `connected_port` 投递，
//! 因此同一会话可同时运行多条 remote 隧道。

use std::collections::HashMap;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::sync::{Arc, Mutex as StdMutex};

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio::task::{JoinHandle, JoinSet};
use uuid::Uuid;

use crate::connection::{ForwardedConn, SshSession};
use crate::error::{Error, Result};

/// 隧道类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TunnelKind {
    /// 本地转发：本地监听 → 远端目标
    Local,
    /// 远端转发：远端监听 → 本地目标
    Remote,
    /// 动态转发：本地 SOCKS5 代理
    Dynamic,
}

/// 隧道配置（前端 JSON 键为 snake_case，与字段名一致）
///
/// 字段语义按 [`TunnelKind`] 区分：
/// - `local`：`local_host:local_port` 本地监听，`remote_host:remote_port` 远端目标
/// - `remote`：`remote_host:remote_port` 远端监听（port 0 = 服务器分配），
///   `local_host:local_port` 本地目标
/// - `dynamic`：`local_host:local_port` 本地 SOCKS5 监听，remote_* 不使用
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TunnelConfig {
    /// 隧道 ID（前端可预生成；为空时由后端生成 UUID）
    #[serde(default)]
    pub id: String,
    /// 隧道类型
    pub kind: TunnelKind,
    /// 本地地址（见上方语义说明）
    pub local_host: String,
    /// 本地端口
    pub local_port: u16,
    /// 远端地址（见上方语义说明；remote 转发时 0 端口会被回填为分配端口）
    pub remote_host: String,
    /// 远端端口
    pub remote_port: u16,
}

/// 隧道运行状态
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TunnelStatus {
    /// 运行中
    Running,
    /// 已停止（用户停止 / 会话断开级联）
    Stopped,
    /// 运行期失败（监听异常 / 会话断开导致转发通道关闭）
    Failed(String),
}

impl TunnelStatus {
    /// 序列化用的状态串（前端按此展示）
    pub fn as_str(&self) -> &'static str {
        match self {
            TunnelStatus::Running => "running",
            TunnelStatus::Stopped => "stopped",
            TunnelStatus::Failed(_) => "failed",
        }
    }
}

/// 隧道状态变更通知（desktop 层注入：emit `tunnel_event` 给前端）
pub type TunnelNotify = Arc<dyn Fn(String, String, TunnelStatus) + Send + Sync>;

/// 隧道列表项（`tunnel_list` 返回）
#[derive(Debug, Clone, Serialize)]
pub struct TunnelInfo {
    /// 隧道 ID
    pub id: String,
    /// 所属会话 ID
    pub session_id: String,
    /// 隧道配置（remote 转发端口 0 时已回填分配端口）
    pub config: TunnelConfig,
    /// 状态串（running / stopped / failed）
    pub state: String,
    /// 失败原因（仅 failed 时有值）
    pub error: Option<String>,
}

/// 运行中隧道的句柄
pub struct TunnelHandle {
    /// 所属会话 ID
    session_id: String,
    /// 隧道配置
    config: TunnelConfig,
    /// 运行状态（accept 循环失败时由 task 自更新）
    state: Arc<StdMutex<TunnelStatus>>,
    /// 停止信号（优雅退出：让 remote 隧道先 cancel_tcpip_forward）
    shutdown: Option<oneshot::Sender<()>>,
    /// 隧道主循环 task
    task: JoinHandle<()>,
}

impl TunnelHandle {
    /// 当前状态快照
    fn status(&self) -> TunnelStatus {
        self.state.lock().unwrap().clone()
    }

    /// 停止隧道：先发停止信号（优雅清理），超时后兜底 abort
    async fn stop(mut self) {
        *self.state.lock().unwrap() = TunnelStatus::Stopped;
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
        // 优雅退出窗口：remote 隧道需要在这段时间内发出 cancel_tcpip_forward
        let _ = tokio::time::timeout(std::time::Duration::from_secs(2), &mut self.task).await;
        self.task.abort();
    }

    fn info(&self) -> TunnelInfo {
        let status = self.status();
        let error = match &status {
            TunnelStatus::Failed(e) => Some(e.clone()),
            _ => None,
        };
        TunnelInfo {
            id: self.config.id.clone(),
            session_id: self.session_id.clone(),
            config: self.config.clone(),
            state: status.as_str().to_string(),
            error,
        }
    }
}

/// 更新状态并按需通知（desktop → 前端 `tunnel_event`）
fn set_state(
    state: &Arc<StdMutex<TunnelStatus>>,
    notify: &Option<TunnelNotify>,
    tunnel_id: &str,
    session_id: &str,
    status: TunnelStatus,
) {
    *state.lock().unwrap() = status.clone();
    if let Some(cb) = notify {
        cb(tunnel_id.to_string(), session_id.to_string(), status);
    }
}

/// SSH 通道 ↔ TCP 连接双向桥接（任一方向 EOF 即结束）
async fn bridge<S>(mut ssh: S, mut tcp: TcpStream, tag: String)
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    match tokio::io::copy_bidirectional(&mut ssh, &mut tcp).await {
        Ok((up, down)) => {
            tracing::debug!(%tag, up, down, "隧道连接桥接结束")
        }
        Err(e) => {
            tracing::debug!(%tag, error = %e, "隧道连接桥接异常结束")
        }
    }
}

/// 隧道管理器（AppState 全局共享，按 tunnel_id 索引）
#[derive(Default)]
pub struct TunnelManager {
    /// tunnel_id → 隧道句柄
    tunnels: Mutex<HashMap<String, TunnelHandle>>,
    /// session_id → remote 转发端口路由表（首条 remote 隧道拉起分发 task 时建立）
    forward_dispatch: Mutex<HashMap<String, Arc<Mutex<HashMap<u16, mpsc::Sender<ForwardedConn>>>>>>,
}

impl TunnelManager {
    /// 创建空管理器
    pub fn new() -> Self {
        Self::default()
    }

    /// 启动隧道，返回 tunnel_id
    ///
    /// 监听绑定 / tcpip-forward 请求在 spawn 前完成，早期失败直接返回错误。
    pub async fn start(
        &self,
        session: Arc<SshSession>,
        mut config: TunnelConfig,
        notify: Option<TunnelNotify>,
    ) -> Result<String> {
        if config.id.is_empty() {
            config.id = Uuid::new_v4().to_string();
        }
        if self.tunnels.lock().await.contains_key(&config.id) {
            return Err(Error::Tunnel(format!("隧道 {} 已存在", config.id)));
        }
        let handle = match config.kind {
            TunnelKind::Local => start_local(session, &config, notify).await?,
            TunnelKind::Remote => self.start_remote(session, &config, notify).await?,
            TunnelKind::Dynamic => start_dynamic(session, &config, notify).await?,
        };
        let id = handle.config.id.clone();
        self.tunnels.lock().await.insert(id.clone(), handle);
        Ok(id)
    }

    /// 停止并移除指定隧道（不存在的 id 视为幂等成功）
    pub async fn stop(&self, tunnel_id: &str) -> Result<()> {
        if let Some(handle) = self.tunnels.lock().await.remove(tunnel_id) {
            handle.stop().await;
        }
        Ok(())
    }

    /// 停止指定会话的全部隧道（会话断开时级联调用）
    pub async fn stop_session(&self, session_id: &str) {
        let ids: Vec<String> = {
            let tunnels = self.tunnels.lock().await;
            tunnels
                .iter()
                .filter(|(_, h)| h.session_id == session_id)
                .map(|(id, _)| id.clone())
                .collect()
        };
        for id in ids {
            let _ = self.stop(&id).await;
        }
        self.forward_dispatch.lock().await.remove(session_id);
    }

    /// 停止全部隧道（应用退出时调用）
    pub async fn stop_all(&self) {
        let handles: Vec<TunnelHandle> = {
            let mut tunnels = self.tunnels.lock().await;
            tunnels.drain().map(|(_, h)| h).collect()
        };
        for handle in handles {
            handle.stop().await;
        }
        self.forward_dispatch.lock().await.clear();
    }

    /// 查询单条隧道信息（停止前取 session_id 等上下文用）
    pub async fn get_info(&self, tunnel_id: &str) -> Option<TunnelInfo> {
        self.tunnels.lock().await.get(tunnel_id).map(TunnelHandle::info)
    }

    /// 列出指定会话的隧道（含运行状态）
    pub async fn list(&self, session_id: &str) -> Vec<TunnelInfo> {
        let tunnels = self.tunnels.lock().await;
        let mut list: Vec<TunnelInfo> = tunnels
            .values()
            .filter(|h| h.session_id == session_id)
            .map(TunnelHandle::info)
            .collect();
        list.sort_by(|a, b| a.id.cmp(&b.id));
        list
    }

    /// 订阅指定会话的 remote 转发入站连接（按监听端口路由）
    ///
    /// 首次调用时取走会话的 forwarded 接收端并拉起按端口分发 task；
    /// 之后同会话的 remote 隧道共享该分发器，各自注册自己的端口队列。
    async fn subscribe_forwarded(
        &self,
        session: &Arc<SshSession>,
        listen_port: u16,
    ) -> Result<mpsc::Receiver<ForwardedConn>> {
        let mut dispatch = self.forward_dispatch.lock().await;
        let table = match dispatch.get(&session.id) {
            Some(table) => table.clone(),
            None => {
                let mut rx = session.take_forwarded_rx().await.ok_or_else(|| {
                    Error::Tunnel("转发接收端已被取走（会话状态异常）".to_string())
                })?;
                let table: Arc<Mutex<HashMap<u16, mpsc::Sender<ForwardedConn>>>> =
                    Arc::new(Mutex::new(HashMap::new()));
                let table_clone = table.clone();
                let session_id = session.id.clone();
                tokio::spawn(async move {
                    while let Some(conn) = rx.recv().await {
                        let port = conn.connected_port as u16;
                        let target = table_clone.lock().await.get(&port).cloned();
                        match target {
                            Some(tx) => {
                                // 隧道停止后发送会失败：连接无人消费，丢弃即可
                                let _ = tx.send(conn).await;
                            }
                            None => {
                                tracing::debug!(
                                    session_id = %session_id,
                                    connected_port = port,
                                    "forwarded-tcpip 无匹配隧道端口，丢弃连接"
                                );
                            }
                        }
                    }
                    tracing::info!(session_id = %session_id, "forwarded 分发循环退出（会话已关闭）");
                });
                dispatch.insert(session.id.clone(), table.clone());
                table
            }
        };
        // 每条隧道独立的接收队列（容量 32，足够突发连接）
        let (tx, rx) = mpsc::channel(32);
        table.lock().await.insert(listen_port, tx);
        Ok(rx)
    }

    /// Remote 转发：远端监听 → 本地目标
    ///
    /// 地址语义（与 OpenSSH `-R` 一致，字段是「远端视角」）：
    /// - `remote_host:remote_port`：请求服务器监听的地址（port 0 = 分配后回填）
    /// - `local_host:local_port`：入站连接从本端 connect 的目标
    async fn start_remote(
        &self,
        session: Arc<SshSession>,
        config: &TunnelConfig,
        notify: Option<TunnelNotify>,
    ) -> Result<TunnelHandle> {
        // 先请求服务器监听（早期失败：权限不足 / AllowTcpForwarding 关闭等）
        let bound_port = session
            .tcpip_forward(&config.remote_host, config.remote_port as u32)
            .await?;
        // 服务器可能分配了不同端口（port=0 场景），回填到配置供前端展示
        let mut config = config.clone();
        config.remote_port = bound_port as u16;

        // 注册端口路由（tcpip_forward 成功后服务器才会投递该端口的连接，无时序竞争）
        let mut inbound = self
            .subscribe_forwarded(&session, bound_port as u16)
            .await
            .map_err(|e| {
                // 路由注册失败时回滚远端监听，避免服务器侧残留
                let session = session.clone();
                let host = config.remote_host.clone();
                tokio::spawn(async move {
                    let _ = session.cancel_tcpip_forward(&host, bound_port).await;
                });
                e
            })?;

        let (shutdown_tx, mut shutdown_rx) = oneshot::channel::<()>();
        let state = Arc::new(StdMutex::new(TunnelStatus::Running));
        let state_task = state.clone();
        let tunnel_id = config.id.clone();
        let session_id = session.id.clone();
        let listen_host = config.remote_host.clone();
        let target_host = config.local_host.clone();
        let target_port = config.local_port;
        let session_task = session.clone();

        let task = tokio::spawn(async move {
            let mut conns = JoinSet::new();
            loop {
                tokio::select! {
                    _ = &mut shutdown_rx => break,
                    conn = inbound.recv() => {
                        match conn {
                            Some(conn) => {
                                let target_host = target_host.clone();
                                let origin = format!(
                                    "{}:{}", conn.originator_address, conn.originator_port
                                );
                                conns.spawn(async move {
                                    // 本地目标不可达时通道随 drop 关闭，对端收到连接重置
                                    match TcpStream::connect((target_host.as_str(), target_port)).await {
                                        Ok(tcp) => {
                                            bridge(conn.channel.into_stream(), tcp, format!("remote→{target_host}:{target_port} ← {origin}")).await;
                                        }
                                        Err(e) => {
                                            tracing::warn!(
                                                target = %format!("{target_host}:{target_port}"),
                                                error = %e,
                                                "remote 隧道本地目标连接失败"
                                            );
                                        }
                                    }
                                });
                            }
                            // 分发循环退出 = 会话已断开：隧道无法再工作，标记失败
                            None => {
                                set_state(
                                    &state_task,
                                    &notify,
                                    &tunnel_id,
                                    &session_id,
                                    TunnelStatus::Failed("SSH 会话已断开".to_string()),
                                );
                                return;
                            }
                        }
                    }
                }
            }
            // 优雅停止：通知服务器撤下远端监听（会话已断开时失败属预期）
            if let Err(e) = session_task.cancel_tcpip_forward(&listen_host, bound_port).await {
                tracing::debug!(tunnel_id = %tunnel_id, error = %e, "cancel_tcpip_forward 未生效（忽略）");
            }
            // conns 随作用域 drop，JoinSet 自动 abort 所有桥接 task
        });

        tracing::info!(
            tunnel_id = %config.id,
            listen = %format!("{}:{bound_port}", config.remote_host),
            target = %format!("{}:{}", config.local_host, config.local_port),
            "remote 隧道已启动"
        );
        Ok(TunnelHandle {
            session_id: session.id.clone(),
            config,
            state,
            shutdown: Some(shutdown_tx),
            task,
        })
    }
}

/// 启动 Local 隧道：本地监听 → `direct-tcpip` 到远端目标
async fn start_local(
    session: Arc<SshSession>,
    config: &TunnelConfig,
    notify: Option<TunnelNotify>,
) -> Result<TunnelHandle> {
    let listener = TcpListener::bind((config.local_host.as_str(), config.local_port))
        .await
        .map_err(|e| {
            Error::Tunnel(format!(
                "本地监听失败 {}:{}: {e}",
                config.local_host, config.local_port
            ))
        })?;
    let target_host = config.remote_host.clone();
    let target_port = config.remote_port;
    Ok(spawn_accept_loop(
        session,
        config,
        notify,
        listener,
        move |session, tcp| {
            let target_host = target_host.clone();
            async move {
                let channel = session.open_direct_tcpip(&target_host, target_port as u32).await?;
                bridge(
                    channel.into_stream(),
                    tcp,
                    format!("local→{target_host}:{target_port}"),
                )
                .await;
                Ok(())
            }
        },
    ))
}

/// 启动 Dynamic 隧道：本地 SOCKS5 代理 → 按请求目标 `direct-tcpip`
async fn start_dynamic(
    session: Arc<SshSession>,
    config: &TunnelConfig,
    notify: Option<TunnelNotify>,
) -> Result<TunnelHandle> {
    let listener = TcpListener::bind((config.local_host.as_str(), config.local_port))
        .await
        .map_err(|e| {
            Error::Tunnel(format!(
                "SOCKS5 监听失败 {}:{}: {e}",
                config.local_host, config.local_port
            ))
        })?;
    Ok(spawn_accept_loop(
        session,
        config,
        notify,
        listener,
        |session, mut tcp| async move {
            match socks5_handshake(&mut tcp).await {
                Ok((host, port)) => {
                    let channel = session.open_direct_tcpip(&host, port as u32).await?;
                    // SSH 通道就绪后再回 SOCKS5 成功应答，客户端随后开始收发数据
                    socks5_reply(&mut tcp, 0x00).await?;
                    bridge(channel.into_stream(), tcp, format!("dynamic→{host}:{port}")).await;
                }
                Err(e) => {
                    tracing::debug!(error = %e, "SOCKS5 握手失败，关闭连接");
                }
            }
            Ok(())
        },
    ))
}

/// 本地监听类隧道（Local / Dynamic）共用的 accept 循环
///
/// select! 在停止信号与 accept 间竞争；每条连接 spawn 独立桥接 task，
/// 隧道停止时 JoinSet drop 连带 abort。accept 持续失败（如 fd 耗尽）
/// 只记日志不退出，避免瞬时错误杀死整条隧道。
/// 本地监听的生命周期独立于 SSH 会话（与 OpenSSH `-L` 一致）：会话断开后
/// 监听仍在，仅新连接的 direct-tcpip 打开失败并逐条断开。
fn spawn_accept_loop<H, Fut>(
    session: Arc<SshSession>,
    config: &TunnelConfig,
    _notify: Option<TunnelNotify>,
    listener: TcpListener,
    handler: H,
) -> TunnelHandle
where
    H: Fn(Arc<SshSession>, TcpStream) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = Result<()>> + Send + 'static,
{
    let (shutdown_tx, mut shutdown_rx) = oneshot::channel::<()>();
    let state = Arc::new(StdMutex::new(TunnelStatus::Running));
    let tunnel_id = config.id.clone();
    let session_id = session.id.clone();
    let listen = format!("{}:{}", config.local_host, config.local_port);

    let task = tokio::spawn(async move {
        let mut conns = JoinSet::new();
        loop {
            tokio::select! {
                _ = &mut shutdown_rx => break,
                accepted = listener.accept() => {
                    match accepted {
                        Ok((tcp, peer)) => {
                            tracing::debug!(tunnel_id = %tunnel_id, %peer, "隧道接受本地连接");
                            conns.spawn(handler(session.clone(), tcp));
                        }
                        Err(e) => {
                            tracing::warn!(tunnel_id = %tunnel_id, error = %e, "隧道 accept 失败");
                        }
                    }
                }
            }
        }
        // conns 随作用域 drop，JoinSet 自动 abort 所有桥接 task
    });

    tracing::info!(tunnel_id = %config.id, %listen, kind = ?config.kind, "本地监听隧道已启动");
    TunnelHandle {
        session_id,
        config: config.clone(),
        state,
        shutdown: Some(shutdown_tx),
        task,
    }
}

/// SOCKS5 握手（最小实现：version 5 / no-auth / CONNECT / IPv4 + DOMAIN + IPv6）
///
/// 成功后返回客户端请求的目标地址；不支持的方法/命令/地址类型直接
/// 回对应错误码并返回 Err（调用方关闭连接）。不引入外部 SOCKS 依赖。
async fn socks5_handshake(stream: &mut TcpStream) -> std::io::Result<(String, u16)> {
    use std::io::{Error as IoError, ErrorKind};

    let bad = |msg: &str| IoError::new(ErrorKind::InvalidData, msg.to_string());

    // 1) 方法协商：VER(5) NMETHODS METHODS —— 只接受 0x00（no-auth）
    let mut head = [0u8; 2];
    stream.read_exact(&mut head).await?;
    if head[0] != 0x05 {
        return Err(bad("非 SOCKS5 协议"));
    }
    let mut methods = vec![0u8; head[1] as usize];
    stream.read_exact(&mut methods).await?;
    if !methods.contains(&0x00) {
        let _ = stream.write_all(&[0x05, 0xFF]).await; // 无可接受方法
        return Err(bad("客户端不支持 no-auth"));
    }
    stream.write_all(&[0x05, 0x00]).await?;

    // 2) 请求：VER CMD RSV ATYP —— 只支持 CONNECT(0x01)
    let mut req = [0u8; 4];
    stream.read_exact(&mut req).await?;
    if req[0] != 0x05 {
        return Err(bad("非 SOCKS5 请求"));
    }
    if req[1] != 0x01 {
        let _ = socks5_reply(stream, 0x07).await; // 不支持的命令（BIND/UDP ASSOCIATE）
        return Err(bad("仅支持 CONNECT 命令"));
    }
    let host = match req[3] {
        0x01 => {
            let mut a = [0u8; 4];
            stream.read_exact(&mut a).await?;
            Ipv4Addr::from(a).to_string()
        }
        0x03 => {
            let mut len = [0u8; 1];
            stream.read_exact(&mut len).await?;
            let mut name = vec![0u8; len[0] as usize];
            stream.read_exact(&mut name).await?;
            String::from_utf8(name).map_err(|_| bad("域名不是合法 UTF-8"))?
        }
        0x04 => {
            let mut a = [0u8; 16];
            stream.read_exact(&mut a).await?;
            Ipv6Addr::from(a).to_string()
        }
        _ => {
            let _ = socks5_reply(stream, 0x08).await; // 不支持的地址类型
            return Err(bad("不支持的地址类型"));
        }
    };
    let mut port = [0u8; 2];
    stream.read_exact(&mut port).await?;
    Ok((host, u16::from_be_bytes(port)))
}

/// SOCKS5 应答（rep：0x00 成功；BND 恒为 0.0.0.0:0，客户端不依赖该值）
async fn socks5_reply(stream: &mut TcpStream, rep: u8) -> std::io::Result<()> {
    stream
        .write_all(&[0x05, rep, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 前端（types/tunnel.ts、ProfileExtra.tunnels）发送的 JSON 形状：
    /// kind 为 snake_case 字符串，字段 snake_case，id 可缺省。
    #[test]
    fn deserializes_frontend_config_shapes() {
        let cfg: TunnelConfig = serde_json::from_str(
            r#"{"id":"t1","kind":"local","local_host":"127.0.0.1","local_port":8080,"remote_host":"10.0.0.2","remote_port":80}"#,
        )
        .unwrap();
        assert!(matches!(cfg.kind, TunnelKind::Local));
        assert_eq!(cfg.local_port, 8080);

        // id 缺省时为空串（由后端生成 UUID）
        let cfg: TunnelConfig = serde_json::from_str(
            r#"{"kind":"dynamic","local_host":"127.0.0.1","local_port":1080,"remote_host":"","remote_port":0}"#,
        )
        .unwrap();
        assert!(matches!(cfg.kind, TunnelKind::Dynamic));
        assert!(cfg.id.is_empty());

        let cfg: TunnelConfig = serde_json::from_str(
            r#"{"id":"t2","kind":"remote","local_host":"127.0.0.1","local_port":3000,"remote_host":"0.0.0.0","remote_port":9000}"#,
        )
        .unwrap();
        assert!(matches!(cfg.kind, TunnelKind::Remote));
    }

    /// TunnelInfo 序列化保持 snake_case（前端按 snake_case 读取）
    #[test]
    fn tunnel_info_serializes_snake_case() {
        let info = TunnelInfo {
            id: "t1".into(),
            session_id: "s1".into(),
            config: TunnelConfig {
                id: "t1".into(),
                kind: TunnelKind::Local,
                local_host: "127.0.0.1".into(),
                local_port: 8080,
                remote_host: "h".into(),
                remote_port: 80,
            },
            state: "running".into(),
            error: None,
        };
        let v = serde_json::to_value(&info).unwrap();
        assert_eq!(v["session_id"], "s1");
        assert_eq!(v["config"]["local_port"], 8080);
        assert_eq!(v["config"]["kind"], "local");
    }
}
