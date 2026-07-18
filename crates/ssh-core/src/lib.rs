//! # ssh-core 领域核心模块
//!
//! SSH 协议领域核心，负责与远端 SSH 服务交互。
//! 本 crate 不依赖任何 UI/应用层框架（Tauri），可独立测试。
//!
//! ## 模块划分
//! - [`connection`]：SSH 连接管理（russh Session 生命周期）
//! - [`terminal`]：PTY 终端会话（channel + 读循环 + 写接口 + 背压）
//! - [`known_hosts`]：主机密钥校验（known_hosts 持久化与策略）
//! - [`sftp`]：SFTP 文件传输客户端
//! - [`tunnel`]：端口转发引擎（Local/Remote/Dynamic）
//! - [`keys`]：密钥管理（生成/导入/Agent Forwarding）
//! - [`error`]：统一错误类型

pub mod connection;
pub mod error;
pub mod keys;
pub mod known_hosts;
pub mod terminal;

// 以下模块在阶段 4/5 实现，暂以 mod 声明占位（可在 src 下后续补全）
// pub mod sftp;
// pub mod tunnel;

pub use error::{Error, Result};
