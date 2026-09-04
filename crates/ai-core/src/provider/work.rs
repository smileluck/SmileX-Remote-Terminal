//! # Work 模式（trait 预留，未实现）
//!
//! 未来实现：自主执行运维任务的 AI Agent（自动诊断、自动修复、function calling 等）。
//! 接口已通过 [`WorkProvider`] 预留，未来实现时无需改 `AgentProvider` trait。

use async_trait::async_trait;

use crate::error::{Error, Result};
use crate::provider::context::Context;
use crate::provider::{AgentMode, AgentProvider, OnToken};

/// Work 模式占位提供者
///
/// 当前所有方法返回 [`Error::WorkNotImplemented`]。
pub struct WorkProvider;

#[async_trait]
impl AgentProvider for WorkProvider {
    fn mode(&self) -> AgentMode {
        AgentMode::Work
    }

    async fn send(&self, _session: &str, _msg: &str, _ctx: &Context, _on_token: OnToken) -> Result<()> {
        // 阶段 5+ 实现：结合 function calling 自主调用 SSH 命令/远程桌面操作
        Err(Error::WorkNotImplemented)
    }

    async fn abort(&self) -> Result<()> {
        Err(Error::WorkNotImplemented)
    }

    async fn clear(&self, _session: &str) -> Result<()> {
        Err(Error::WorkNotImplemented)
    }
}
