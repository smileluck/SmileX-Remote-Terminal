//! # ai-core AI 运维助手领域核心
//!
//! 提供 Chat 模式（已实现）与 Work 模式（trait 预留）的统一抽象。
//!
//! ## 模块划分
//! - [`provider`]：`AgentProvider` trait（Chat/Work 统一抽象）
//!   - `chat`：Chat 模式实现
//!     - `llm`：LLM 客户端（多 Provider 适配）
//!     - `context`：会话上下文采集
//!     - `history`：对话历史
//!   - `work`：Work 模式（trait 预留）
//! - [`error`]：统一错误类型
//!
//! ## 关键设计
//! - `AgentProvider` trait 统一 Chat/Work；Work 未来实现不改 trait
//! - `LlmClient` trait 屏蔽 OpenAI/Claude/Ollama 差异
//! - Chat 发送时可选附带运维上下文（终端最近 N 行输出）

pub mod error;
pub mod provider;

pub use error::{Error, Result};
pub use provider::llm::LlmClient;
pub use provider::{
    context::Context,
    history::{Message, Role},
    AgentMode, AgentProvider, LlmProviderConfig,
};
