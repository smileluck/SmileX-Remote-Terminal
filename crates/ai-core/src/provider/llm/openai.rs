//! # OpenAI 兼容 LLM 客户端
//!
//! 覆盖 OpenAI 官方及兼容协议（DeepSeek/智谱/通义等）。
//! 实现 `/v1/chat/completions` 流式调用 + SSE 解析。

use async_trait::async_trait;

use crate::error::{Error, Result};
use crate::provider::history::Message;
use crate::provider::llm::{LlmClient, LlmProtocol, ThinkWrap};
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

/// 单条 SSE data 的解析结果
enum DeltaOut {
    /// 文本增量；bool 标记是否来自推理字段（reasoning）
    Token(String, bool),
    /// 无关载荷，跳过
    Ignore,
}

/// 解析单条 SSE data JSON
///
/// 兼容各厂商差异：
/// - 推理模型把思考过程放 `delta.reasoning_content`（DeepSeek/Qwen 等）
///   或 `delta.reasoning`（OpenRouter 等），与正文 `delta.content` 区分
/// - 部分兼容服务在流中以 `{"error": {...}}` 对象报错（HTTP 仍是 200），
///   必须显式转为错误，否则会被当作正常流静默截断
fn parse_openai_data(data: &str) -> Result<DeltaOut> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(data) else {
        return Ok(DeltaOut::Ignore);
    };

    if let Some(err) = v.get("error") {
        let msg = err
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("unknown error");
        return Err(Error::LlmApi(format!("流式响应错误: {msg}")));
    }

    let Some(delta) = v
        .get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("delta"))
    else {
        return Ok(DeltaOut::Ignore);
    };

    for key in ["reasoning_content", "reasoning"] {
        if let Some(t) = delta.get(key).and_then(|t| t.as_str()) {
            if !t.is_empty() {
                return Ok(DeltaOut::Token(t.to_string(), true));
            }
        }
    }
    if let Some(t) = delta.get("content").and_then(|t| t.as_str()) {
        if !t.is_empty() {
            return Ok(DeltaOut::Token(t.to_string(), false));
        }
    }
    Ok(DeltaOut::Ignore)
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

        // 解析 SSE 流（简化版：按行读取，提取 data: 中的增量）
        use futures_util::StreamExt;
        let mut stream = response.bytes_stream();
        let mut buf = String::new();
        let mut think = ThinkWrap::default();

        'outer: while let Some(chunk) = stream.next().await {
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
                    break 'outer;
                }

                match parse_openai_data(data)? {
                    DeltaOut::Token(token, is_thinking) => {
                        let wrapped = think.wrap(&token, is_thinking);
                        if !wrapped.is_empty() {
                            on_token(wrapped);
                        }
                    }
                    DeltaOut::Ignore => {}
                }
            }
        }

        // 思考段未闭合（模型没给闭标记就结束）时补上
        if let Some(close) = think.close() {
            on_token(close);
        }

        Ok(())
    }
}
