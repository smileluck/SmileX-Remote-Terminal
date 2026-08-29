//! # 监控采样调度器
//!
//! 按 sessionId 维护定时采样 task：
//! - 周期性通过 `SshSession::exec` 执行 [`COLLECT_SCRIPT`] 采集指标
//! - 与上一次原始值做差值换算出 CPU% / 网络 bps
//! - 通过 Tauri event `monitor_metrics` 推送给前端
//! - `CancellationToken` 取消（会话断开 / monitor_stop）
//!
//! exec 往返耗时同时作为会话延迟（latency_ms）上报。

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tauri::Emitter;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use ssh_core::connection::SessionManager as SshSessionManager;

use crate::events::{MonitorDisk, MonitorMetricsPayload};
use crate::monitor::metrics::{self, RawMetrics};

/// exec 超时（慢速网络 / 高负载主机兜底）
const EXEC_TIMEOUT: Duration = Duration::from_secs(10);

/// 采样调度器（Tauri managed state）
#[derive(Default)]
pub struct MonitorSampler {
    /// sessionId → 取消令牌
    tasks: Mutex<HashMap<String, CancellationToken>>,
}

impl MonitorSampler {
    pub fn new() -> Self {
        Self::default()
    }

    /// 启动某会话的采样（已在采样中则先停止旧 task 再重启，用于调整间隔）
    pub async fn start(
        &self,
        app: tauri::AppHandle,
        ssh_manager: Arc<SshSessionManager>,
        session_id: String,
        interval_ms: u64,
    ) {
        self.stop(&session_id).await;

        let token = CancellationToken::new();
        self.tasks
            .lock()
            .await
            .insert(session_id.clone(), token.clone());

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(
                interval_ms.clamp(1000, 60_000),
            ));
            // 首个 tick 立即触发，实现首采不等待
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            let mut prev: Option<RawMetrics> = None;
            loop {
                tokio::select! {
                    _ = token.cancelled() => break,
                    _ = interval.tick() => {}
                }

                let session = match ssh_manager.get(&session_id).await {
                    Some(s) => s,
                    None => break, // 会话已移除
                };

                let started = std::time::Instant::now();
                let raw = tokio::time::timeout(
                    EXEC_TIMEOUT,
                    session.exec(metrics::COLLECT_SCRIPT),
                )
                .await;
                let latency_ms = started.elapsed().as_millis() as u64;

                let raw = match raw {
                    Ok(Ok(out)) => metrics::parse_output(&out),
                    Ok(Err(e)) => {
                        tracing::warn!(session_id = %session_id, error = %e, "监控采集失败");
                        continue;
                    }
                    Err(_) => {
                        tracing::warn!(session_id = %session_id, "监控采集超时");
                        continue;
                    }
                };

                // CPU%：优先差值，macOS 降级用直接值
                let cpu_percent = raw
                    .cpu_percent_direct
                    .or_else(|| {
                        prev.as_ref()
                            .and_then(|p| raw.cpu.percent_between(&p.cpu))
                    });

                // 网络 bps：与上次累计值做差 / 间隔
                let (net_rx_bps, net_tx_bps) = {
                    let (rx, tx) = raw.net_totals();
                    match prev.as_ref().map(|p| p.net_totals()) {
                        Some((prx, ptx)) if rx >= prx && tx >= ptx => {
                            let secs = interval_ms as f64 / 1000.0;
                            (((rx - prx) as f64 / secs), ((tx - ptx) as f64 / secs))
                        }
                        _ => (0.0, 0.0),
                    }
                };

                let payload = MonitorMetricsPayload {
                    session_id: session_id.clone(),
                    timestamp_ms: chrono::Utc::now().timestamp_millis() as u64,
                    latency_ms: Some(latency_ms),
                    cpu_percent,
                    mem_total_bytes: raw.mem.total,
                    mem_used_bytes: raw.mem.used(),
                    mem_percent: raw.mem.used_percent(),
                    swap_total_bytes: raw.mem.swap_total,
                    swap_used_bytes: raw.mem.swap_total.saturating_sub(raw.mem.swap_free),
                    load1: raw.load1,
                    uptime_s: raw.uptime_s,
                    net_rx_bps,
                    net_tx_bps,
                    disks: raw
                        .disks
                        .iter()
                        .map(|d| MonitorDisk {
                            mount: d.mount.clone(),
                            total_kb: d.total_kb,
                            used_kb: d.used_kb,
                            used_percent: d.used_percent(),
                        })
                        .collect(),
                    error: None,
                };

                if app.emit("monitor_metrics", payload).is_err() {
                    break;
                }
                prev = Some(raw);
            }
            tracing::debug!(session_id = %session_id, "监控采样 task 退出");
        });
    }

    /// 停止某会话的采样（不存在时静默）
    pub async fn stop(&self, session_id: &str) {
        if let Some(token) = self.tasks.lock().await.remove(session_id) {
            token.cancel();
        }
    }

    /// 当前采样中的 sessionId 列表
    pub async fn list(&self) -> Vec<String> {
        self.tasks.lock().await.keys().cloned().collect()
    }

    /// 停止所有采样（应用退出时调用）
    pub async fn stop_all(&self) {
        let tokens: Vec<_> = self.tasks.lock().await.drain().map(|(_, t)| t).collect();
        for t in tokens {
            t.cancel();
        }
    }
}
