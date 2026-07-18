//! # Tauri Event 推送 payload 定义
//!
//! 后端 emit 事件的 payload 结构，前端据此反序列化。

use serde::{Deserialize, Serialize};

/// `terminal_output` 事件 payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalOutputPayload {
    /// 会话 ID
    pub session_id: String,
    /// 终端数据（原始字节，前端转字符串/Uint8Array）
    pub data: Vec<u8>,
}

/// `desktop_frame` 事件 payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DesktopFramePayload {
    /// 会话 ID
    pub session_id: String,
    /// 帧序号
    pub seq: u64,
    /// 宽
    pub width: u32,
    /// 高
    pub height: u32,
    /// RGBA 像素
    pub rgba: Vec<u8>,
    /// 是否关键帧
    pub key_frame: bool,
}

/// `ai_token` 事件 payload（流式 token）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiTokenPayload {
    /// 会话 ID
    pub session_id: String,
    /// token 文本
    pub token: String,
}

/// `ai_done` 事件 payload（响应结束）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiDonePayload {
    /// 会话 ID
    pub session_id: String,
    /// 是否成功（无错误）
    pub success: bool,
    /// 错误信息（失败时）
    pub error: Option<String>,
}
