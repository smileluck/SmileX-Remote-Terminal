//! # PTY 终端会话
//!
//! 负责从 russh channel 持续读取输出并通过 **有界 mpsc channel** 推送给应用层。
//!
//! ## 背压机制（防止大输出 OOM）
//!
//! 详见架构 §2.6。核心：读循环写入容量 64 的 `tokio::sync::mpsc`，
//! 满则阻塞；应用层消费不及时会反向施压 SSH 协议层。
//!
//! ```text
//! 远端 SSH → russh channel → [读循环] → mpsc(64) → 应用层 → Tauri event → xterm.js
//! ```
//!
//! **注**：当前为阶段 1 骨架，读循环在阶段 2 基于 russh 0.45 真实 API 补全。

use tokio::sync::mpsc;

use crate::connection::PtyHandle;

/// 终端输出的一块数据（从 russh channel 读出的字节）
#[derive(Debug, Clone)]
pub struct TerminalChunk {
    /// 远端推送的字节（可能是 ANSI 转义序列、可见字符等）
    pub data: Vec<u8>,
}

/// 终端流句柄
///
/// 持有一个 mpsc receiver（消费读循环产物）和 PTY 的 channel_id（用于写入）。
/// drop 时自动结束读循环。
pub struct TerminalStream {
    /// 接收端（应用层消费）
    pub rx: mpsc::Receiver<TerminalChunk>,
    /// PTY channel id（应用层通过 SshSession::write 写入）
    pub channel_id: u32,
}

impl TerminalStream {
    /// 基于 PTY 句柄创建终端流
    ///
    /// 阶段 1：创建 channel 但不拉起真实读循环（阶段 2 接入 russh channel.wait()）。
    pub fn start(pty: PtyHandle) -> Self {
        // 有界 channel：容量 64 帧，背压关键
        let (_tx, rx) = mpsc::channel::<TerminalChunk>(64);

        // TODO(阶段 2): 拉起读循环
        // tokio::spawn(async move {
        //     Self::read_loop(pty.channel, tx).await;
        // });

        Self {
            rx,
            channel_id: pty.channel_id,
        }
    }

    /// 读循环（阶段 2 实现真实版本）
    ///
    /// 持续从 russh channel 读消息，推送到 mpsc，满则阻塞（背压）。
    #[allow(dead_code)]
    async fn read_loop(_channel_id: u32, _tx: mpsc::Sender<TerminalChunk>) {
        // 阶段 2 基于 russh::Channel<russh::client::Msg> 的 wait() 实现
        // loop {
        //     match channel.wait().await {
        //         Some(ChannelMsg::Data { data }) => { tx.send(...).await }
        //         Some(ChannelMsg::Eof | ChannelMsg::Close) | None => break,
        //         _ => {}
        //     }
        // }
    }

    /// 主动停止（阶段 1 无 task 需停止）
    pub async fn stop(&mut self) {}
}
