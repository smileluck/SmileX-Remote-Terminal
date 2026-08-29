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

/// 单挂载点磁盘信息（`monitor_metrics` 内嵌）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorDisk {
    /// 挂载点（如 `/`）
    pub mount: String,
    /// 总量（KB）
    pub total_kb: u64,
    /// 已用（KB）
    pub used_kb: u64,
    /// 使用率（0–100）
    pub used_percent: f64,
}

/// `monitor_metrics` 事件 payload（一次采样结果）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorMetricsPayload {
    /// 会话 ID
    pub session_id: String,
    /// 采样时间戳（毫秒）
    pub timestamp_ms: u64,
    /// exec 往返延迟（毫秒；采集失败时为 None）
    pub latency_ms: Option<u64>,
    /// CPU 使用率（0–100；首轮差值不可用时为 None）
    pub cpu_percent: Option<f64>,
    /// 内存总量（字节）
    pub mem_total_bytes: u64,
    /// 内存已用（字节）
    pub mem_used_bytes: u64,
    /// 内存使用率（0–100）
    pub mem_percent: f64,
    /// Swap 总量（字节）
    pub swap_total_bytes: u64,
    /// Swap 已用（字节）
    pub swap_used_bytes: u64,
    /// 1 分钟负载
    pub load1: f64,
    /// 主机运行时长（秒）
    pub uptime_s: u64,
    /// 下行速率（bytes/s）
    pub net_rx_bps: f64,
    /// 上行速率（bytes/s）
    pub net_tx_bps: f64,
    /// 磁盘列表
    pub disks: Vec<MonitorDisk>,
    /// 采集错误（预留：用于推送采样失败通知）
    pub error: Option<String>,
}

/// `alert_fired` 事件 payload（告警规则触发）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertFiredPayload {
    /// 规则 ID
    pub rule_id: String,
    /// 规则名称
    pub name: String,
    /// 会话 ID
    pub session_id: String,
    /// 指标名
    pub metric: String,
    /// 当前值
    pub value: f64,
    /// 阈值
    pub threshold: f64,
    /// 比较符
    pub op: String,
    /// 触发时间戳（毫秒）
    pub timestamp_ms: u64,
}
