//! # LLM 客户端统一抽象
//!
//! `LlmClient` trait 屏蔽不同协议（OpenAI 兼容 / Anthropic / Ollama）差异，
//! 各协议适配器实现此 trait。
//!
//! 厂商预设（[`LlmProvider`]）与传输协议（[`LlmProtocol`]）分离：
//! - 厂商预设决定 UI 展示、默认端点与模型列表（如 `zhipu` / `deepseek`）
//! - 接入方式（[`LlmAuthMode`]）区分「按量 API」与「Coding Plan 订阅」：
//!   国内厂商的 Coding Plan 普遍通过 Anthropic 兼容端点提供
//! - 实际客户端由 [`resolve_protocol`] 按「预设 + 接入方式」选择

pub mod anthropic;
pub mod ollama;
pub mod openai;

use async_trait::async_trait;

use crate::error::Result;
use crate::provider::history::Message;

/// LLM 厂商预设类型
///
/// 序列化为小写字符串（与前端 `types/settings.ts` 约定一致）：
/// `OpenAi` → `"openai"` / `Claude` → `"claude"` / `Zhipu` → `"zhipu"` 等
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LlmProvider {
    /// OpenAI 官方及任意 OpenAI 兼容自定义端点
    OpenAi,
    /// Anthropic Claude（官方原生协议）
    Claude,
    /// 本地 Ollama
    Ollama,
    /// 智谱 GLM（BigModel 开放平台）
    Zhipu,
    /// DeepSeek
    DeepSeek,
    /// Kimi / Moonshot（月之暗面）
    Moonshot,
    /// 通义千问（阿里云百炼）
    Qwen,
    /// MiniMax
    Minimax,
}

/// 接入方式：区分按量计费 API 与 Coding Plan 订阅
///
/// 同一厂商的两种方式端点不同、协议不同、API Key 常不通用
/// （如 Kimi Coding Plan 为独立订阅，Key 与按量 API 分属不同平台签发）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LlmAuthMode {
    /// 按量计费 API（走 OpenAI 兼容端点）
    Api,
    /// Coding Plan 订阅（走 Anthropic 兼容端点）
    CodingPlan,
}

/// 传输协议（决定使用哪个客户端适配器）
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LlmProtocol {
    /// OpenAI 兼容：`{base}/chat/completions`
    OpenAiCompatible,
    /// Anthropic：`{base}/v1/messages`
    Anthropic,
    /// Ollama：`{base}/api/chat`
    Ollama,
}

/// 按厂商预设 + 接入方式推导传输协议
///
/// - Claude 官方 → Anthropic 原生协议
/// - 国内厂商 Coding Plan 订阅 → Anthropic 兼容端点；
///   按量 API（含未指定接入方式）→ OpenAI 兼容端点
pub fn resolve_protocol(provider: LlmProvider, auth_mode: Option<LlmAuthMode>) -> LlmProtocol {
    match provider {
        LlmProvider::Claude => LlmProtocol::Anthropic,
        LlmProvider::Ollama => LlmProtocol::Ollama,
        LlmProvider::OpenAi => LlmProtocol::OpenAiCompatible,
        LlmProvider::Zhipu
        | LlmProvider::DeepSeek
        | LlmProvider::Moonshot
        | LlmProvider::Qwen
        | LlmProvider::Minimax => match auth_mode {
            Some(LlmAuthMode::CodingPlan) => LlmProtocol::Anthropic,
            Some(LlmAuthMode::Api) | None => LlmProtocol::OpenAiCompatible,
        },
    }
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

    /// 传输协议
    fn protocol(&self) -> LlmProtocol;
}

/// 按 [`crate::LlmProviderConfig`] 创建 LLM 客户端
///
/// 由 `resolve_protocol` 决定协议后分发到对应适配器。
pub fn create_client(config: &crate::LlmProviderConfig) -> std::sync::Arc<dyn LlmClient> {
    match resolve_protocol(config.provider, config.auth_mode) {
        LlmProtocol::OpenAiCompatible => {
            std::sync::Arc::new(openai::OpenAiClient::new(config.clone()))
        }
        LlmProtocol::Anthropic => {
            std::sync::Arc::new(anthropic::AnthropicClient::new(config.clone()))
        }
        LlmProtocol::Ollama => std::sync::Arc::new(ollama::OllamaClient::new(config.clone())),
    }
}
