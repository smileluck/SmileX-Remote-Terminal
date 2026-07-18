//! # PTY 终端会话
//!
//! 负责从 russh channel 持续读取输出并通过 **有界 mpsc channel** 推送给应用层。
//!
//! ## 背压机制（防止大输出 OOM）
//!
//! 详见架构 §2.6。核心：读循环写入容量 64 的 `tokio::sync::mpsc`，
//! 满则阻塞；应用层消费不及时会反向施压 SSH 协议层（TCP 窗口收缩）。
//!
//! ```text
//! 远端 SSH → russh channel → [读循环] → mpsc(64) → 应用层 → Tauri event → xterm.js
//! ```
//!
//! ## 读循环退出条件
//! - `ChannelMsg::Eof`：远端关闭写端
//! - `ChannelMsg::Close`：SSH 通道完全关闭
//! - `channel.wait()` 返回 `None`：channel 底层 sender drop
//!
//! ## JoinHandle 生命周期
//! 读循环 task 句柄保存在 `TerminalStream`，drop 时自动 abort（避免泄漏）。

use russh::{ChannelMsg, Channel, client};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::connection::PtyHandle;

/// 终端输出的一块数据（从 russh channel 读出的字节）
#[derive(Debug, Clone)]
pub struct TerminalChunk {
    /// 远端推送的字节（可能是 ANSI 转义序列、可见字符等）
    pub data: Vec<u8>,
}

/// 终端流句柄
///
/// 持有：
/// - mpsc receiver（应用层消费读循环产物）
/// - PTY 的 channel_id（应用层通过 `SshSession::write/resize` 操作）
/// - 读循环 JoinHandle（drop 时自动 abort）
///
/// 应用层应在独立 task 中循环 `rx.recv().await`，消费完自然结束。
pub struct TerminalStream {
    /// 接收端（应用层消费）
    pub rx: mpsc::Receiver<TerminalChunk>,
    /// PTY channel id（应用层通过 SshSession::write/resize 操作）
    pub channel_id: russh::ChannelId,
    /// 读循环 task 句柄（drop TerminalStream 时自动 abort）
    _read_task: Option<JoinHandle<()>>,
}

impl TerminalStream {
    /// 基于 PTY 句柄创建终端流
    ///
    /// 内部拉起独立 tokio task 驱动读循环：
    /// 1. 创建容量 64 的有界 mpsc（背压关键）
    /// 2. `tokio::spawn` 拉起读循环，持有 channel + sender
    /// 3. 返回 TerminalStream，持有 rx + JoinHandle
    ///
    /// 应用层 drop TerminalStream → JoinHandle drop → tokio 自动 abort 读循环 task。
    pub fn start(pty: PtyHandle) -> Self {
        // 有界 channel：容量 64 帧，背压关键
        let (tx, rx) = mpsc::channel::<TerminalChunk>(64);

        let channel_id = pty.channel_id;
        let channel = pty.channel;

        // 拉起读循环（独立 task，持有 channel + tx）
        let read_task = tokio::spawn(Self::read_loop(channel, tx));

        Self {
            rx,
            channel_id,
            _read_task: Some(read_task),
        }
    }

    /// 读循环
    ///
    /// 持续从 russh channel 读消息：
    /// - `ChannelMsg::Data { data }`：远端 stdout/stderr 输出，转换为 `TerminalChunk` 推送
    /// - `ChannelMsg::ExtendedData { data, ext }`：stderr（ext=1），合并推送
    /// - `ChannelMsg::Eof | Close` 或 `None`：channel 结束，退出循环
    ///
    /// 推送失败（应用层消费端 drop）时主动退出，避免悬挂 task。
    async fn read_loop(mut channel: Channel<client::Msg>, tx: mpsc::Sender<TerminalChunk>) {
        // 转为 u32 用于日志（ChannelId 未实现 tracing::Value，但实现了 Display/Into<u32>）
        let channel_id: u32 = channel.id().into();
        tracing::debug!(channel_id, "PTY 读循环启动");

        loop {
            // 等待下一条 channel 消息（None 表示 channel 已关闭）
            let msg = match channel.wait().await {
                Some(msg) => msg,
                None => {
                    tracing::debug!(channel_id, "channel 已关闭（wait 返回 None），读循环退出");
                    break;
                }
            };

            match msg {
                // 标准 stdout 数据
                ChannelMsg::Data { data } => {
                    let chunk = TerminalChunk {
                        data: data.to_vec(),
                    };
                    // 推送到 mpsc：满则 await 阻塞（背压生效）
                    if tx.send(chunk).await.is_err() {
                        tracing::debug!(
                            channel_id,
                            "应用层 receiver 已 drop，读循环退出"
                        );
                        break;
                    }
                }
                // 扩展数据（通常是 stderr，ext=1）
                ChannelMsg::ExtendedData { data, .. } => {
                    let chunk = TerminalChunk {
                        data: data.to_vec(),
                    };
                    if tx.send(chunk).await.is_err() {
                        tracing::debug!(
                            channel_id,
                            "应用层 receiver 已 drop，读循环退出（extended data）"
                        );
                        break;
                    }
                }
                // 远端关闭写端
                ChannelMsg::Eof => {
                    tracing::debug!(channel_id, "收到 EOF，读循环退出");
                    break;
                }
                // 通道完全关闭
                ChannelMsg::Close => {
                    tracing::debug!(channel_id, "收到 Close，读循环退出");
                    break;
                }
                // 其他消息（WindowAdjusted / ExitStatus / Success / Failure 等）忽略
                _ => {}
            }
        }

        tracing::debug!(channel_id, "PTY 读循环结束");
    }

    /// 主动停止读循环（应用层显式断开时调用）
    ///
    /// 注：通常 drop TerminalStream 即可自动 abort，此方法用于需要显式控制的场景。
    pub async fn stop(&mut self) {
        if let Some(handle) = self._read_task.take() {
            handle.abort();
            // 等待 abort 生效（忽略 JoinError）
            let _ = handle.await;
        }
    }
}

impl Drop for TerminalStream {
    fn drop(&mut self) {
        // 兜底：drop 时若 task 还未 take 走，主动 abort
        if let Some(handle) = self._read_task.take() {
            handle.abort();
        }
    }
}
