//! # 监控模块
//!
//! - [`metrics`]：远程主机指标采集脚本与解析
//! - [`sampler`]：按 sessionId 的定时采样调度器（Tauri event 推送）

pub mod metrics;
pub mod sampler;
