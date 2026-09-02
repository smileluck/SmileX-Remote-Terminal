//! # Tauri Event 推送 payload 定义
//!
//! 后端 emit 事件的 payload 结构，前端据此反序列化。

use serde::{Deserialize, Serialize};

/// `terminal_output` 事件 payload
///
/// 前端（app/src/types/session.ts）读 camelCase `sessionId`，
/// 经 `Channel::send` 序列化不做自动命名转换，须显式 rename。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalOutputPayload {
    /// 会话 ID
    pub session_id: String,
    /// 终端数据（原始字节，前端转字符串/Uint8Array）
    pub data: Vec<u8>,
}

/// `desktop_frame` 事件 payload
///
/// 前端（app/src/types/desktop.ts）读 camelCase `sessionId`/`keyFrame`。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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
///
/// 前端（app/src/types/ai.ts、stores/agent.ts）读 camelCase `sessionId`。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiTokenPayload {
    /// 会话 ID
    pub session_id: String,
    /// token 文本
    pub token: String,
}

/// `ai_done` 事件 payload（响应结束）
///
/// 前端（app/src/types/ai.ts、stores/agent.ts）读 camelCase `sessionId`。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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

#[cfg(test)]
mod tests {
    use super::*;

    /// 前端读取这些 payload 的字段名是 camelCase（ipc::Channel / emit 序列化
    /// 不做自动命名转换），锁定字段命名，防止再次出现 session_id/sessionId
    /// 不匹配导致输出被静默丢弃。
    #[test]
    fn payloads_serialize_to_camel_case() {
        let terminal = TerminalOutputPayload {
            session_id: "s1".into(),
            data: vec![1, 2],
        };
        let v = serde_json::to_value(&terminal).unwrap();
        assert_eq!(v["sessionId"], "s1");
        assert!(v.get("session_id").is_none());

        let frame = DesktopFramePayload {
            session_id: "s1".into(),
            seq: 1,
            width: 2,
            height: 3,
            rgba: vec![0],
            key_frame: true,
        };
        let v = serde_json::to_value(&frame).unwrap();
        assert_eq!(v["sessionId"], "s1");
        assert_eq!(v["keyFrame"], true);

        let token = AiTokenPayload {
            session_id: "chat-1".into(),
            token: "hi".into(),
        };
        let v = serde_json::to_value(&token).unwrap();
        assert_eq!(v["sessionId"], "chat-1");

        let done = AiDonePayload {
            session_id: "chat-1".into(),
            success: true,
            error: None,
        };
        let v = serde_json::to_value(&done).unwrap();
        assert_eq!(v["sessionId"], "chat-1");
    }

    /// monitor/alert 事件前端读 snake_case（stores/monitor.ts 读 session_id），
    /// 两侧约定一致，不得改成 camelCase。
    #[test]
    fn monitor_payloads_stay_snake_case() {
        let metrics = MonitorMetricsPayload {
            session_id: "s1".into(),
            timestamp_ms: 1,
            latency_ms: None,
            cpu_percent: None,
            mem_total_bytes: 1,
            mem_used_bytes: 1,
            mem_percent: 0.0,
            swap_total_bytes: 0,
            swap_used_bytes: 0,
            load1: 0.0,
            uptime_s: 1,
            net_rx_bps: 0.0,
            net_tx_bps: 0.0,
            disks: vec![],
            error: None,
        };
        let v = serde_json::to_value(&metrics).unwrap();
        assert_eq!(v["session_id"], "s1");
    }
}
