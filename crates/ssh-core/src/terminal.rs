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
//! ## select! + 控制通道（阶段 4）
//!
//! russh 0.45 限制：`Channel::window_change(&self)` 与 `Channel::wait(&mut self)` 共享
//! channel 内部状态，**不能跨 task 并发**。唯一安全的做法是在同一个 task 内串行执行。
//!
//! 解决方案：将读循环改造为 `tokio::select!`，同时监听：
//! - `channel.wait()`：读取远端消息
//! - `control_rx.recv()`：接收应用层控制指令（如 Resize）
//!
//! 收到 Resize 时，在 select! 分支内**同步**调用 `channel.window_change(cols, rows).await`，
//! 保证与 wait() 串行。`biased` 优先处理控制指令，避免被持续的数据流饿死。
//!
//! ## 读循环退出条件
//! - `ChannelMsg::Eof`：远端关闭写端
//! - `ChannelMsg::Close`：SSH 通道完全关闭
//! - `channel.wait()` 返回 `None`：channel 底层 sender drop
//! - `control_rx.recv()` 返回 `None`：应用层 drop 了 TerminalControl
//!
//! ## JoinHandle 生命周期
//! 读循环 task 句柄保存在 `TerminalStream`，drop 时自动 abort（避免泄漏）。

use russh::{Channel, ChannelMsg, client};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::connection::PtyHandle;
use crate::error::{Error, Result};

/// 终端输出的一块数据（从 russh channel 读出的字节）
#[derive(Debug, Clone)]
pub struct TerminalChunk {
    /// 远端推送的字节（可能是 ANSI 转义序列、可见字符等）
    pub data: Vec<u8>,
}

/// 终端控制指令（应用层 → 读循环）
///
/// 通过 [`TerminalControl`] 投递到读循环内部，
/// 在 `select!` 分支内串行执行，避免与 `channel.wait()` 竞争。
#[derive(Debug, Clone)]
pub enum ControlMsg {
    /// 调整 PTY 窗口尺寸（发送 SSH window-change 请求）
    Resize {
        /// 新列数（字符宽度）
        cols: u32,
        /// 新行数（字符高度）
        rows: u32,
    },
}

/// 终端控制句柄
///
/// 通过此句柄向读循环发送控制指令（目前支持 Resize）。
///
/// - **线程安全**：内部 `mpsc::Sender` 可克隆
/// - **生命周期**：读循环退出后发送会返回错误，应用层应处理此情况
/// - **设计**：clone 廉价，可在多个 Tauri command 间共享
#[derive(Clone)]
pub struct TerminalControl {
    /// 指向读循环的控制 channel sender
    tx: mpsc::Sender<ControlMsg>,
}

impl TerminalControl {
    /// 发送 resize 指令到读循环
    ///
    /// 注意：此方法仅投递指令，实际 window-change 是否成功需读循环日志确认。
    /// 失败（读循环已退出）时返回错误。
    ///
    /// # 参数
    /// - `cols`：新列数（字符宽度）
    /// - `rows`：新行数（字符高度）
    pub async fn resize(&self, cols: u32, rows: u32) -> Result<()> {
        self.tx
            .send(ControlMsg::Resize { cols, rows })
            .await
            .map_err(|_| Error::Terminal("终端读循环已退出，无法发送 resize 指令".into()))?;
        Ok(())
    }
}

/// 终端流句柄
///
/// 持有：
/// - mpsc receiver（应用层消费读循环产物）
/// - PTY 的 channel_id（应用层通过 `SshSession::write` 操作）
/// - 读循环 JoinHandle（drop 时自动 abort）
///
/// 应用层应在独立 task 中循环 `rx.recv().await`，消费完自然结束。
pub struct TerminalStream {
    /// 接收端（应用层消费）
    pub rx: mpsc::Receiver<TerminalChunk>,
    /// PTY channel id（应用层通过 SshSession::write 操作）
    pub channel_id: russh::ChannelId,
    /// 读循环 task 句柄（drop TerminalStream 时自动 abort）
    _read_task: Option<JoinHandle<()>>,
}

impl TerminalStream {
    /// 基于 PTY 句柄创建终端流
    ///
    /// 内部拉起独立 tokio task 驱动 select! 读循环：
    /// 1. 创建容量 64 的输出 mpsc（背压关键）
    /// 2. 创建容量 16 的控制 mpsc（避免 resize 阻塞）
    /// 3. `tokio::spawn` 拉起 select! 读循环
    /// 4. 返回 `(TerminalStream, TerminalControl)`
    ///
    /// 应用层应将 `TerminalControl` 注册到 AppState，供 resize 等命令使用。
    /// drop TerminalStream → JoinHandle drop → tokio 自动 abort 读循环 task。
    pub fn start(pty: PtyHandle) -> (Self, TerminalControl) {
        // 输出 channel：容量 64 帧，背压关键
        let (tx, rx) = mpsc::channel::<TerminalChunk>(64);
        // 控制 channel：容量 16，避免 resize 指令被输出阻塞
        // （window_change 是低频指令，16 足够缓冲）
        let (ctrl_tx, ctrl_rx) = mpsc::channel::<ControlMsg>(16);

        let channel_id = pty.channel_id;
        let channel = pty.channel;

        // 拉起 select! 读循环
        let read_task = tokio::spawn(Self::read_loop(channel, tx, ctrl_rx));

        let stream = Self {
            rx,
            channel_id,
            _read_task: Some(read_task),
        };
        let control = TerminalControl { tx: ctrl_tx };

        (stream, control)
    }

    /// select! 读循环
    ///
    /// 同时监听两个源：
    /// 1. **控制通道**（优先，`biased`）：收到 Resize 时串行调用 `channel.window_change()`
    /// 2. **russh channel**：读取 Data/ExtendedData/Eof/Close 等消息
    ///
    /// 关键点：
    /// - `biased` 保证控制指令优先处理（避免被高频率数据流饿死）
    /// - `channel.wait()` 与 `channel.window_change()` 在同一 task 内串行，
    ///   避开 russh 0.45 的 &self/&mut self 互斥问题
    /// - 任一源关闭即退出循环
    async fn read_loop(
        mut channel: Channel<client::Msg>,
        tx: mpsc::Sender<TerminalChunk>,
        mut ctrl_rx: mpsc::Receiver<ControlMsg>,
    ) {
        // 转为 u32 用于日志（ChannelId 未实现 tracing::Value，但实现了 Display/Into<u32>）
        let channel_id: u32 = channel.id().into();
        tracing::debug!(channel_id, "PTY select! 读循环启动");

        loop {
            tokio::select! {
                // 优先处理控制指令（biased 排序）
                biased;

                // 控制通道：Resize 等指令
                ctrl = ctrl_rx.recv() => {
                    match ctrl {
                        Some(ControlMsg::Resize { cols, rows }) => {
                            tracing::debug!(
                                channel_id, cols, rows,
                                "收到 resize 指令，调用 channel.window_change"
                            );
                            // 在 select! 分支内同步调用，与 wait() 串行
                            // pix_width / pix_height 传 0（字符终端不使用像素尺寸）
                            if let Err(e) = channel.window_change(cols, rows, 0, 0).await {
                                tracing::warn!(
                                    channel_id, error = %e,
                                    "window_change 请求失败"
                                );
                            }
                        }
                        None => {
                            tracing::debug!(
                                channel_id,
                                "控制通道已关闭（TerminalControl 全部 drop），读循环退出"
                            );
                            break;
                        }
                    }
                }

                // russh channel：远端消息
                msg = channel.wait() => {
                    let msg = match msg {
                        Some(msg) => msg,
                        None => {
                            tracing::debug!(
                                channel_id,
                                "channel 已关闭（wait 返回 None），读循环退出"
                            );
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
            }
        }

        tracing::debug!(channel_id, "PTY select! 读循环结束");
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
