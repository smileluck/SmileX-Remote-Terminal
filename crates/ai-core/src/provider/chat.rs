//! # Chat 模式实现
//!
//! 已实现：用户提问 → LLM 流式回答；可选附带运维上下文。

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

/// Chat 模式提供者
pub struct ChatProvider {
    /// LLM 客户端（懒初始化，配置变更时重建）
    client: AsyncMutex<Option<Arc<dyn LlmClient>>>,
    /// 当前 LLM 配置
    config: AsyncMutex<Option<LlmProviderConfig>>,
    /// 对话历史
    history: Mutex<History>,
    /// 取消标志（用户中断）
    cancelled: AsyncMutex<bool>,
}

impl ChatProvider {
    /// 创建 Chat 提供者
    pub fn new() -> Self {
        Self {
            client: AsyncMutex::new(None),
            config: AsyncMutex::new(None),
            history: Mutex::new(History::new(DEFAULT_MAX_ROUNDS)),
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

    async fn send(&self, msg: &str, ctx: &Context, on_token: OnToken) -> Result<()> {
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
        let history_msgs = self.history.lock().unwrap().messages().to_vec();
        messages.extend(history_msgs);

        // 3) 当前用户问题
        let user_msg = Message::user(msg);
        messages.push(user_msg.clone());

        // 调用前先把 user 消息入历史
        self.history.lock().unwrap().push(user_msg);

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
            let partial = assistant_content.lock().unwrap().clone();
            if !partial.is_empty() {
                self.history.lock().unwrap().push(Message::assistant(partial));
            }
            return Err(Error::Cancelled);
        }

        match result {
            Ok(()) => {
                let content = assistant_content.lock().unwrap().clone();
                self.history.lock().unwrap().push(Message::assistant(content));
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    async fn abort(&self) -> Result<()> {
        *self.cancelled.lock().await = true;
        Ok(())
    }

    async fn clear(&self) -> Result<()> {
        self.history.lock().unwrap().clear();
        Ok(())
    }
}
