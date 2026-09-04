//! # Chat 模式实现
//!
//! 已实现：用户提问 → LLM 流式回答；可选附带运维上下文。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tokio::sync::Mutex as AsyncMutex;

use crate::error::{Error, Result};
use crate::provider::context::Context;
use crate::provider::history::{History, Message};
use crate::provider::llm::{create_client, LlmClient};
use crate::provider::{AgentMode, AgentProvider, OnToken};
use crate::LlmProviderConfig;

/// 默认保留对话轮数
const DEFAULT_MAX_ROUNDS: usize = 10;

/// 剥离内容中的 `<think>…</think>` / `<thinking>…</thinking>` 段
///
/// 推理模型的思考过程只用于展示（流式推送前端折叠渲染），
/// 不入对话历史，避免污染下一轮上下文（也符合 DeepSeek 等对
/// reasoning 内容不应回传的要求）。未闭合的思考段（生成被中断）
/// 一并丢弃。模型也常原生输出这些标记，统一在此清理。
/// pub：命令层从持久化消息恢复历史时同样需要剥离。
pub fn strip_think(content: &str) -> String {
    fn find_open(s: &str) -> Option<(&str, usize)> {
        // 返回 (开标记, 开标记结束位置)；先出现的优先
        let a = s.find("<think>").map(|i| ("<think>", i + 7));
        let b = s.find("<thinking>").map(|i| ("<thinking>", i + 10));
        match (a, b) {
            (Some(a), Some(b)) => Some(if a.1 <= b.1 { a } else { b }),
            (a, None) => a,
            (None, b) => b,
        }
    }

    let mut out = String::with_capacity(content.len());
    let mut rest = content;
    while let Some((open, open_end)) = find_open(rest) {
        out.push_str(&rest[..open_end - open.len()]);
        let after = &rest[open_end..];
        let close = if open == "<think>" { "</think>" } else { "</thinking>" };
        match after.find(close) {
            Some(i) => rest = &after[i + close.len()..],
            // 未闭合：其余内容整体属于思考段，丢弃
            None => return out,
        }
    }
    out.push_str(rest);
    out
}

/// Chat 模式提供者
pub struct ChatProvider {
    /// LLM 客户端（懒初始化，配置变更时重建）
    client: AsyncMutex<Option<Arc<dyn LlmClient>>>,
    /// 当前 LLM 配置
    config: AsyncMutex<Option<LlmProviderConfig>>,
    /// 对话历史（按会话隔离，session → 历史）
    histories: Mutex<HashMap<String, History>>,
    /// 取消标志（用户中断）
    cancelled: AsyncMutex<bool>,
}

impl ChatProvider {
    /// 创建 Chat 提供者
    pub fn new() -> Self {
        Self {
            client: AsyncMutex::new(None),
            config: AsyncMutex::new(None),
            histories: Mutex::new(HashMap::new()),
            cancelled: AsyncMutex::new(false),
        }
    }

    /// 更新 LLM 配置（用户在设置页变更 Provider 时调用）
    pub async fn update_config(&self, config: LlmProviderConfig) {
        let client = create_client(&config);
        *self.client.lock().await = Some(client);
        *self.config.lock().await = Some(config);
    }

    /// 是否已配置 LLM
    pub async fn is_configured(&self) -> bool {
        self.config.lock().await.is_some()
    }

    /// 指定会话是否已有内存历史（用于判断是否需要从持久化恢复）
    pub fn has_history(&self, session: &str) -> bool {
        self.histories.lock().unwrap().contains_key(session)
    }

    /// 写入指定会话的历史（仅在无条目时生效，幂等）
    ///
    /// 用于应用重启后从持久化消息恢复 LLM 上下文；
    /// 各 History 自身仍按轮数上限截断。
    pub fn seed_history(&self, session: &str, msgs: Vec<Message>) {
        let mut map = self.histories.lock().unwrap();
        map.entry(session.to_string()).or_insert_with(|| {
            let mut h = History::new(DEFAULT_MAX_ROUNDS);
            for m in msgs {
                h.push(m);
            }
            h
        });
    }
}

impl Default for ChatProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AgentProvider for ChatProvider {
    fn mode(&self) -> AgentMode {
        AgentMode::Chat
    }

    async fn send(&self, session: &str, msg: &str, ctx: &Context, on_token: OnToken) -> Result<()> {
        // 取 LLM 客户端
        let client_guard = self.client.lock().await;
        let client = client_guard
            .as_ref()
            .ok_or_else(|| Error::LlmConfig("LLM 尚未配置，请在设置页添加 Provider".into()))?
            .clone();
        drop(client_guard);

        // 重置取消标志
        *self.cancelled.lock().await = false;

        // 组装消息列表
        let mut messages: Vec<Message> = Vec::new();

        // 1) 运维上下文作为 system prompt（若用户勾选）
        if let Some(sys) = ctx.to_system_prompt() {
            messages.push(Message::system(sys));
        }

        // 2) 历史对话
        let history_msgs = self
            .histories
            .lock()
            .unwrap()
            .entry(session.to_string())
            .or_insert_with(|| History::new(DEFAULT_MAX_ROUNDS))
            .messages()
            .to_vec();
        messages.extend(history_msgs);

        // 3) 当前用户问题
        let user_msg = Message::user(msg);
        messages.push(user_msg.clone());

        // 调用前先把 user 消息入历史
        self.histories
            .lock()
            .unwrap()
            .entry(session.to_string())
            .or_insert_with(|| History::new(DEFAULT_MAX_ROUNDS))
            .push(user_msg);

        // 流式收集 assistant 回复（并行触发 UI 推送）
        let assistant_content = Arc::new(Mutex::new(String::new()));
        let assistant_clone = assistant_content.clone();
        let merged = Arc::new(move |token: String| {
            assistant_clone.lock().unwrap().push_str(&token);
            on_token(token);
        }) as Arc<dyn Fn(String) + Send + Sync>;

        // 调用 LLM（传 Arc clone）
        let result = client.chat_stream(&messages, merged).await;

        // 检查是否被取消
        if *self.cancelled.lock().await {
            let partial = strip_think(&assistant_content.lock().unwrap().clone());
            if !partial.is_empty() {
                self.push_assistant(session, partial);
            }
            return Err(Error::Cancelled);
        }

        match result {
            Ok(()) => {
                let content = strip_think(&assistant_content.lock().unwrap().clone());
                self.push_assistant(session, content);
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    async fn abort(&self) -> Result<()> {
        *self.cancelled.lock().await = true;
        Ok(())
    }

    async fn clear(&self, session: &str) -> Result<()> {
        self.histories.lock().unwrap().remove(session);
        Ok(())
    }
}

impl ChatProvider {
    /// 追加 assistant 回复入指定会话的历史
    fn push_assistant(&self, session: &str, content: String) {
        self.histories
            .lock()
            .unwrap()
            .entry(session.to_string())
            .or_insert_with(|| History::new(DEFAULT_MAX_ROUNDS))
            .push(Message::assistant(content));
    }
}

#[cfg(test)]
mod tests {
    use super::strip_think;

    #[test]
    fn test_strip_think() {
        // 常规闭合段
        assert_eq!(strip_think("<think>\n推导\n</think>\n\n答案"), "\n\n答案");
        // thinking 变体
        assert_eq!(strip_think("<thinking>a</thinking>b"), "b");
        // 多段
        assert_eq!(strip_think("a<think>1</think>b<think>2</think>c"), "abc");
        // 未闭合：整体丢弃
        assert_eq!(strip_think("答案<think>被打断的思考"), "答案");
        // 无 think 标记原样返回
        assert_eq!(strip_think("普通回答"), "普通回答");
        // 原生闭合标签在正文中间
        assert_eq!(strip_think("<think>x</think>```run\nls\n```"), "```run\nls\n```");
    }
}
