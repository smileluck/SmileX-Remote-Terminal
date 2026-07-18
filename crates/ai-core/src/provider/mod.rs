//! # AI Agent 提供者统一抽象
//!
//! `AgentProvider` trait 统一 Chat 与 Work 两种模式。
//! 已实现 `chat::ChatProvider`；`work::WorkProvider` trait 预留。

pub mod chat;
pub mod context;
pub mod history;
pub mod llm;
pub mod work;

use async_trait::async_trait;

use crate::error::Result;
use crate::provider::context::Context;

/// Token 回调（流式响应，需 Send + Sync 以跨线程）
pub type OnToken = std::sync::Arc<dyn Fn(String) + Send + Sync>;

/// Agent 模式枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentMode {
    /// Chat：即问即答 + 运维上下文（已实现）
    Chat,
    /// Work：自主执行运维任务（trait 预留）
    Work,
}

/// AI Agent 提供者 trait —— Chat/Work 统一抽象
///
/// 已实现 `ChatProvider`；`WorkProvider` 未来实现，无需改此 trait。
#[async_trait]
pub trait AgentProvider: Send + Sync {
    /// 模式标识
    fn mode(&self) -> AgentMode;

    /// 发送消息（流式响应通过回调推送 token）
    ///
    /// `ctx` 携带运维上下文（终端最近 N 行）；`on_token` 每个 token 触发一次。
    async fn send(&self, msg: &str, ctx: &Context, on_token: OnToken) -> Result<()>;

    /// 中断当前生成
    async fn abort(&self) -> Result<()>;

    /// 清空对话历史
    async fn clear(&self) -> Result<()>;
}

/// LLM Provider 配置
///
/// 由前端设置页传入，API Key 由应用层从 OS Keyring 取。
///
/// serde `rename_all = "camelCase"`：JSON 字段使用 camelCase（`baseUrl` / `apiKey`），
/// 与 JavaScript/Vue 社区惯例一致。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmProviderConfig {
    /// Provider 类型
    pub provider: llm::LlmProvider,
    /// 模型名（如 "gpt-4o" / "claude-3-5-sonnet" / "qwen2.5:7b"）
    pub model: String,
    /// API Base URL（留空用 Provider 默认）
    pub base_url: Option<String>,
    /// API Key
    pub api_key: Option<String>,
    /// 是否流式
    #[serde(default = "default_true")]
    pub stream: bool,
}

fn default_true() -> bool {
    true
}
