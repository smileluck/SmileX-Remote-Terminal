//! # 对话历史管理
//!
//! 每个会话维护独立的多轮对话历史（内存）。
//! 超长历史自动截断（保留最近 N 轮 + 系统提示）。

/// 消息角色
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
}

/// 一条对话消息
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

impl Message {
    pub fn system(content: impl Into<String>) -> Self {
        Self { role: Role::System, content: content.into() }
    }
    pub fn user(content: impl Into<String>) -> Self {
        Self { role: Role::User, content: content.into() }
    }
    pub fn assistant(content: impl Into<String>) -> Self {
        Self { role: Role::Assistant, content: content.into() }
    }
}

/// 对话历史（单会话）
#[derive(Debug, Clone, Default)]
pub struct History {
    /// 消息列表
    messages: Vec<Message>,
    /// 保留的最大轮数（一问一答算一轮 = 2 条消息）
    max_rounds: usize,
}

impl History {
    /// 创建指定轮数上限的历史
    pub fn new(max_rounds: usize) -> Self {
        Self {
            messages: Vec::new(),
            max_rounds,
        }
    }

    /// 追加一条消息
    pub fn push(&mut self, msg: Message) {
        self.messages.push(msg);
        self.truncate();
    }

    /// 清空
    pub fn clear(&mut self) {
        self.messages.clear();
    }

    /// 取所有消息（供 LLM 调用）
    pub fn messages(&self) -> &[Message] {
        &self.messages
    }

    /// 截断：保留系统提示 + 最近 N 轮
    fn truncate(&mut self) {
        if self.messages.len() <= self.max_rounds * 2 {
            return;
        }
        // 保留开头的 system 消息（若有）
        let sys_count = self
            .messages
            .iter()
            .take_while(|m| m.role == Role::System)
            .count();
        let keep = self.max_rounds * 2;
        let start = self.messages.len().saturating_sub(keep);
        let sys_part: Vec<Message> = self.messages.drain(..sys_count).collect();
        self.messages.drain(..start.saturating_sub(sys_count));
        self.messages.splice(..0, sys_part);
    }
}
