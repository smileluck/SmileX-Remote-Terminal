//! # Anthropic 兼容 LLM 客户端
//!
//! 覆盖 Anthropic 官方 Messages API 及国内厂商 Coding Plan 提供的
//! Anthropic 兼容端点（智谱 / Kimi / DeepSeek / 百炼 / MiniMax 等）。
//! 实现 `/v1/messages` 流式调用 + SSE 解析。
//!
//! 鉴权：先按官方 SDK 约定发 `x-api-key`；若返回 401 则改用
//! `Authorization: Bearer` 重试一次（部分 Coding Plan 端点仅认
//! Claude Code 的 `ANTHROPIC_AUTH_TOKEN` Bearer 约定，官方端点则禁止两种头叠加）。

use async_trait::async_trait;

use crate::error::{Error, Result};
use crate::provider::history::{Message, Role};
use crate::provider::llm::{LlmClient, LlmProtocol};
use crate::LlmProviderConfig;

/// Anthropic Messages API 要求必填 max_tokens
const DEFAULT_MAX_TOKENS: u32 = 4096;
/// 协议版本（部分兼容端点会忽略此头，官方必需）
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// Anthropic 兼容客户端
pub struct AnthropicClient {
    config: LlmProviderConfig,
    http: reqwest::Client,
}

impl AnthropicClient {
    /// 创建客户端
    pub fn new(config: LlmProviderConfig) -> Self {
        Self {
            config,
            http: reqwest::Client::new(),
        }
    }

    /// Base URL（默认 Anthropic 官方）
    fn base_url(&self) -> String {
        self.config
            .base_url
            .clone()
            .unwrap_or_else(|| "https://api.anthropic.com".to_string())
    }

    /// 角色转 Anthropic 字符串（system 已抽出，只余 user/assistant）
    fn role_str(role: &Role) -> &'static str {
        match role {
            Role::System | Role::User => "user",
            Role::Assistant => "assistant",
        }
    }

    /// 发送请求（`bearer = true` 时改用 Bearer 鉴权，用于 401 回退）
    async fn send_request(
        &self,
        url: &str,
        body: &serde_json::Value,
        api_key: Option<&str>,
        bearer: bool,
    ) -> Result<reqwest::Response> {
        let mut request = self
            .http
            .post(url)
            .json(body)
            .header("anthropic-version", ANTHROPIC_VERSION);
        if let Some(key) = api_key {
            if bearer {
                request = request.bearer_auth(key);
            } else {
                request = request.header("x-api-key", key);
            }
        }
        request
            .send()
            .await
            .map_err(|e| Error::LlmApi(format!("请求失败: {e}")))
    }
}

/// 从消息列表分离 system 提示与对话消息
///
/// Anthropic 协议中 system 是请求体顶层参数，messages 只允许
/// user/assistant 交替；system 可出现在任意位置（拼接合并），
/// 相邻同角色的对话消息合并为一条。
///
/// 返回 `(system, 对话消息)`；对话消息为空时补一条空 user（协议要求至少一条）。
fn split_system(messages: &[Message]) -> (Option<String>, Vec<(Role, String)>) {
    let mut system_parts: Vec<&str> = Vec::new();
    let mut chat: Vec<(Role, String)> = Vec::new();

    for m in messages {
        match m.role {
            Role::System => system_parts.push(m.content.as_str()),
            Role::User | Role::Assistant => {
                if let Some(last) = chat.last_mut() {
                    if last.0 == m.role {
                        last.1.push_str("\n\n");
                        last.1.push_str(&m.content);
                        continue;
                    }
                }
                chat.push((m.role, m.content.clone()));
            }
        }
    }

    if chat.is_empty() {
        chat.push((Role::User, " ".to_string()));
    }

    let system = if system_parts.is_empty() {
        None
    } else {
        Some(system_parts.join("\n\n"))
    };
    (system, chat)
}

/// SSE `data:` 载荷的解析结果
#[derive(Debug, PartialEq, Eq)]
enum SseDelta {
    /// 增量文本（content_block_delta 的 delta.text）
    Text(String),
    /// 消息结束（message_stop）
    Stop,
    /// 无关事件，跳过
    Ignore,
}

/// 解析单条 SSE data JSON，提取增量文本 / 结束标记
fn parse_sse_data(data: &str) -> SseDelta {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(data) else {
        return SseDelta::Ignore;
    };
    match v.get("type").and_then(|t| t.as_str()) {
        Some("content_block_delta") => {
            // text_delta 事件的结构为 {"delta": {"type": "text_delta", "text": "..."}}
            match v
                .get("delta")
                .and_then(|d| d.get("text"))
                .and_then(|t| t.as_str())
            {
                Some(text) if !text.is_empty() => SseDelta::Text(text.to_string()),
                _ => SseDelta::Ignore,
            }
        }
        Some("message_stop") => SseDelta::Stop,
        _ => SseDelta::Ignore,
    }
}

#[async_trait]
impl LlmClient for AnthropicClient {
    fn protocol(&self) -> LlmProtocol {
        LlmProtocol::Anthropic
    }

    async fn chat_stream(
        &self,
        messages: &[Message],
        on_token: std::sync::Arc<dyn Fn(String) + Send + Sync>,
    ) -> Result<()> {
        let url = format!("{}/v1/messages", self.base_url());

        let (system, chat) = split_system(messages);
        let mut body = serde_json::json!({
            "model": self.config.model,
            "max_tokens": DEFAULT_MAX_TOKENS,
            "stream": true,
            "messages": chat.iter().map(|(role, content)| {
                serde_json::json!({
                    "role": Self::role_str(role),
                    "content": content,
                })
            }).collect::<Vec<_>>(),
        });
        if let Some(sys) = &system {
            body["system"] = serde_json::json!(sys);
        }

        let api_key = self.config.api_key.as_deref();

        // 首选官方 SDK 的 x-api-key 约定；401 时回退 Bearer（Coding Plan 端点约定）
        let mut response = self.send_request(&url, &body, api_key, false).await?;
        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            response = self.send_request(&url, &body, api_key, true).await?;
        }

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(Error::LlmApi(format!("HTTP {status}: {text}")));
        }

        // 解析 SSE 流：按行读取，提取 data: 中 content_block_delta 的文本
        use futures_util::StreamExt;
        let mut stream = response.bytes_stream();
        let mut buf = String::new();

        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| Error::LlmApi(format!("读取流失败: {e}")))?;
            buf.push_str(&String::from_utf8_lossy(&chunk));

            while let Some(pos) = buf.find('\n') {
                let line: String = buf.drain(..=pos).collect();
                let line = line.trim();

                if line.is_empty() || line.starts_with(':') || line.starts_with("event:") {
                    continue;
                }
                if !line.starts_with("data:") {
                    continue;
                }
                let data = line.trim_start_matches("data:").trim();

                match parse_sse_data(data) {
                    SseDelta::Text(text) => on_token(text),
                    SseDelta::Stop => return Ok(()),
                    SseDelta::Ignore => {}
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(role: Role, content: &str) -> Message {
        Message {
            role,
            content: content.to_string(),
        }
    }

    #[test]
    fn test_split_system_extracts_and_merges() {
        // system 在开头，正常分离
        let (system, chat) = split_system(&[
            msg(Role::System, "你是助手"),
            msg(Role::User, "你好"),
            msg(Role::Assistant, "在"),
            msg(Role::User, "继续"),
        ]);
        assert_eq!(system.as_deref(), Some("你是助手"));
        assert_eq!(chat.len(), 3);
        assert_eq!(chat[0].1, "你好");

        // 相邻同角色合并
        let (_, chat) = split_system(&[
            msg(Role::User, "第一段"),
            msg(Role::User, "第二段"),
            msg(Role::Assistant, "回复"),
        ]);
        assert_eq!(chat.len(), 2);
        assert_eq!(chat[0].1, "第一段\n\n第二段");

        // system 在中间也抽出；全 system 时补空 user
        let (system, chat) = split_system(&[msg(Role::User, "问"), msg(Role::System, "中途插入")]);
        assert_eq!(system.as_deref(), Some("中途插入"));
        assert_eq!(chat.len(), 1);

        let (_, chat) = split_system(&[msg(Role::System, "只有系统")]);
        assert_eq!(chat.len(), 1);
        assert_eq!(chat[0].0, Role::User);
    }

    #[test]
    fn test_parse_sse_data() {
        // text_delta 提取
        assert_eq!(
            parse_sse_data(r#"{"type":"content_block_delta","delta":{"type":"text_delta","text":"你好"}}"#),
            SseDelta::Text("你好".to_string())
        );
        // message_stop 结束
        assert_eq!(
            parse_sse_data(r#"{"type":"message_stop"}"#),
            SseDelta::Stop
        );
        // 其他事件忽略
        assert_eq!(
            parse_sse_data(r#"{"type":"message_start","message":{}}"#),
            SseDelta::Ignore
        );
        // thinking_delta（无 text 字段）忽略
        assert_eq!(
            parse_sse_data(r#"{"type":"content_block_delta","delta":{"type":"thinking_delta","thinking":"..."}}"#),
            SseDelta::Ignore
        );
        // 空 text / 非法 JSON 忽略
        assert_eq!(
            parse_sse_data(r#"{"type":"content_block_delta","delta":{"text":""}}"#),
            SseDelta::Ignore
        );
        assert_eq!(parse_sse_data("not json"), SseDelta::Ignore);
    }
}
