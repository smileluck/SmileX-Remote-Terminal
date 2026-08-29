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
    /// 监控指标摘要（应用层采集后传入，如 "CPU 82.5% / MEM 71.2% / load1 3.4"）
    #[serde(default)]
    pub metrics_summary: Option<String>,
    /// 用户是否勾选附带上下文
    pub include_context: bool,
}

impl Context {
    /// 创建"无上下文"的通用问答 Context
    pub fn no_context() -> Self {
        Self {
            session_id: None,
            terminal_output: None,
            metrics_summary: None,
            include_context: false,
        }
    }

    /// 创建"附带上下文"的 Context
    pub fn with_terminal(session_id: String, terminal_output: String) -> Self {
        Self {
            session_id: Some(session_id),
            terminal_output: Some(terminal_output),
            metrics_summary: None,
            include_context: true,
        }
    }

    /// 组装进 system prompt 的文本
    pub fn to_system_prompt(&self) -> Option<String> {
        if !self.include_context {
            return None;
        }
        let mut prompt = String::from(
            "你是一位资深运维工程师助手。请结合以下用户当前环境上下文回答问题。",
        );
        let mut has_any = false;
        if let Some(m) = self.metrics_summary.as_ref() {
            prompt.push_str(&format!("\n\n## 监控指标\n{}", m));
            has_any = true;
        }
        if let Some(output) = self.terminal_output.as_ref() {
            prompt.push_str(&format!("\n\n## 终端最近输出\n```\n{}\n```", output));
            has_any = true;
        }
        if has_any {
            Some(prompt)
        } else {
            None
        }
    }
}
