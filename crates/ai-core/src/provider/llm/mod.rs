//! # LLM 客户端统一抽象
//!
//! `LlmClient` trait 屏蔽不同 Provider（OpenAI/Claude/Ollama）差异。
//! 各 Provider 适配器实现此 trait。

pub mod ollama;
pub mod openai;

use async_trait::async_trait;

use crate::error::Result;
use crate::provider::history::Message;

/// LLM Provider 类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum LlmProvider {
    /// OpenAI 兼容（含 DeepSeek/智谱/通义等兼容协议）
    OpenAi,
    /// Anthropic Claude
    Claude,
    /// 本地 Ollama
    Ollama,
}

/// LLM 客户端 trait
#[async_trait]
pub trait LlmClient: Send + Sync {
    /// 流式对话
    ///
    /// `on_token` 每收到一个 token 触发一次。需 `Send + Sync` 以支持异步跨线程。
    async fn chat_stream(
        &self,
        messages: &[Message],
        on_token: std::sync::Arc<dyn Fn(String) + Send + Sync>,
    ) -> Result<()>;

    /// Provider 类型
    fn provider(&self) -> LlmProvider;
}

/// 按 [`crate::LlmProviderConfig`] 创建 LLM 客户端
pub fn create_client(config: &crate::LlmProviderConfig) -> std::sync::Arc<dyn LlmClient> {
    match config.provider {
        LlmProvider::OpenAi => std::sync::Arc::new(openai::OpenAiClient::new(config.clone())),
        LlmProvider::Ollama => std::sync::Arc::new(ollama::OllamaClient::new(config.clone())),
        LlmProvider::Claude => {
            // Claude 适配器阶段 2 补全，MVP 先用 OpenAI 兼容占位
            std::sync::Arc::new(openai::OpenAiClient::new(config.clone()))
        }
    }
}
