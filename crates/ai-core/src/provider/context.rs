//! # 会话上下文采集
//!
//! Chat 模式可附带当前 SSH 会话的终端最近 N 行输出作为运维上下文。
//! 实际采集由应用层（desktop）通过 SessionRegistry 取 ssh-core 的终端 buffer 实现。

/// 运维上下文（Chat 模式用）
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Context {
    /// 关联的 SSH 会话 ID（None 表示纯通用问答）
    pub session_id: Option<String>,
    /// 终端最近 N 行输出（应用层采集后传入）
    pub terminal_output: Option<String>,
    /// 用户是否勾选附带上下文
    pub include_context: bool,
}

impl Context {
    /// 创建"无上下文"的通用问答 Context
    pub fn no_context() -> Self {
        Self {
            session_id: None,
            terminal_output: None,
            include_context: false,
        }
    }

    /// 创建"附带上下文"的 Context
    pub fn with_terminal(session_id: String, terminal_output: String) -> Self {
        Self {
            session_id: Some(session_id),
            terminal_output: Some(terminal_output),
            include_context: true,
        }
    }

    /// 组装进 system prompt 的文本
    pub fn to_system_prompt(&self) -> Option<String> {
        if !self.include_context {
            return None;
        }
        self.terminal_output.as_ref().map(|output| {
            format!(
                "你是一位资深运维工程师助手。以下是用户当前 SSH 终端的最近输出，请结合该上下文回答用户问题：\n\n```\n{}\n```",
                output
            )
        })
    }
}
