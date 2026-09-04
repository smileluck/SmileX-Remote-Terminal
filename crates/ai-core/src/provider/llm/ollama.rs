//! # Ollama 本地 LLM 客户端
//!
//! 调用本地 Ollama REST API（默认 http://localhost:11434），数据不出本机。

use async_trait::async_trait;

use crate::error::{Error, Result};
use crate::provider::history::Message;
use crate::provider::llm::{LlmClient, LlmProtocol};
use crate::LlmProviderConfig;

/// Ollama 客户端
pub struct OllamaClient {
    config: LlmProviderConfig,
    http: reqwest::Client,
}

impl OllamaClient {
    pub fn new(config: LlmProviderConfig) -> Self {
        Self {
            config,
            http: reqwest::Client::new(),
        }
    }

    fn base_url(&self) -> String {
        self.config
            .base_url
            .clone()
            .unwrap_or_else(|| "http://localhost:11434".to_string())
    }

    fn role_str(role: &crate::provider::history::Role) -> &'static str {
        use crate::provider::history::Role;
        match role {
            Role::System => "system",
            Role::User => "user",
            Role::Assistant => "assistant",
        }
    }
}

#[async_trait]
impl LlmClient for OllamaClient {
    fn protocol(&self) -> LlmProtocol {
        LlmProtocol::Ollama
    }

    async fn chat_stream(
        &self,
        messages: &[Message],
        on_token: std::sync::Arc<dyn Fn(String) + Send + Sync>,
    ) -> Result<()> {
        let url = format!("{}/api/chat", self.base_url());

        let body = serde_json::json!({
            "model": self.config.model,
            "messages": messages.iter().map(|m| {
                serde_json::json!({
                    "role": Self::role_str(&m.role),
                    "content": m.content,
                })
            }).collect::<Vec<_>>(),
            "stream": true,
        });

        let response = self
            .http
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::LlmApi(format!("Ollama 请求失败: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(Error::LlmApi(format!("Ollama HTTP {status}: {text}")));
        }

        // Ollama 流式：每行一个 JSON 对象，message.content 字段为增量
        use futures_util::StreamExt;
        let mut stream = response.bytes_stream();
        let mut buf = String::new();

        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| Error::LlmApi(format!("Ollama 读流失败: {e}")))?;
            buf.push_str(&String::from_utf8_lossy(&chunk));

            while let Some(pos) = buf.find('\n') {
                let line: String = buf.drain(..=pos).collect();
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
                    if let Some(content) = v
                        .get("message")
                        .and_then(|m| m.get("content"))
                        .and_then(|c| c.as_str())
                    {
                        if !content.is_empty() {
                            on_token(content.to_string());
                        }
                    }
                    // done 字段标识结束
                    if v.get("done").and_then(|d| d.as_bool()).unwrap_or(false) {
                        return Ok(());
                    }
                }
            }
        }

        Ok(())
    }
}
