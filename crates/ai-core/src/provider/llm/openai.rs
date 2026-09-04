//! # OpenAI 兼容 LLM 客户端
//!
//! 覆盖 OpenAI 官方及兼容协议（DeepSeek/智谱/通义等）。
//! 实现 `/v1/chat/completions` 流式调用 + SSE 解析。

use async_trait::async_trait;

use crate::error::{Error, Result};
use crate::provider::history::Message;
use crate::provider::llm::{LlmClient, LlmProtocol};
use crate::LlmProviderConfig;

/// OpenAI 兼容客户端
pub struct OpenAiClient {
    config: LlmProviderConfig,
    http: reqwest::Client,
}

impl OpenAiClient {
    /// 创建客户端
    pub fn new(config: LlmProviderConfig) -> Self {
        Self {
            config,
            http: reqwest::Client::new(),
        }
    }

    /// Base URL（默认 OpenAI 官方）
    fn base_url(&self) -> String {
        self.config
            .base_url
            .clone()
            .unwrap_or_else(|| "https://api.openai.com/v1".to_string())
    }

    /// 角色转 OpenAI 字符串
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
impl LlmClient for OpenAiClient {
    fn protocol(&self) -> LlmProtocol {
        LlmProtocol::OpenAiCompatible
    }

    async fn chat_stream(
        &self,
        messages: &[Message],
        on_token: std::sync::Arc<dyn Fn(String) + Send + Sync>,
    ) -> Result<()> {
        let url = format!("{}/chat/completions", self.base_url());

        // 构建请求体
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

        let mut request = self.http.post(&url).json(&body);
        if let Some(key) = &self.config.api_key {
            request = request.bearer_auth(key);
        }

        // 发送请求
        let response = request
            .send()
            .await
            .map_err(|e| Error::LlmApi(format!("请求失败: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(Error::LlmApi(format!("HTTP {status}: {text}")));
        }

        // 解析 SSE 流（简化版：按行读取，提取 data: 中的 content）
        use futures_util::StreamExt;
        let mut stream = response.bytes_stream();
        let mut buf = String::new();

        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| Error::LlmApi(format!("读取流失败: {e}")))?;
            buf.push_str(&String::from_utf8_lossy(&chunk));

            // 按行处理
            while let Some(pos) = buf.find('\n') {
                let line: String = buf.drain(..=pos).collect();
                let line = line.trim();

                if line.is_empty() || line.starts_with(':') {
                    continue;
                }
                if !line.starts_with("data:") {
                    continue;
                }
                let data = line.trim_start_matches("data:").trim();
                if data == "[DONE]" {
                    return Ok(());
                }

                // 解析 JSON
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(data) {
                    if let Some(delta) = v
                        .get("choices")
                        .and_then(|c| c.get(0))
                        .and_then(|c| c.get("delta"))
                        .and_then(|d| d.get("content"))
                        .and_then(|c| c.as_str())
                    {
                        if !delta.is_empty() {
                            on_token(delta.to_string());
                        }
                    }
                }
            }
        }

        Ok(())
    }
}
