//! # Tauri Commands 处理器
//!
//! 按 SSH/远程桌面/AI/通用 分组，对接前端 invoke 调用。

pub mod ai;
pub mod alert;
pub mod common;
pub mod desktop;
pub mod llm_profile;
pub mod monitor;
pub mod session;
pub mod session_profile;
pub mod sftp;
pub mod snippet;
