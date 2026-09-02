//! # 会话上下文采集
//!
//! Agent（Chat）模式可附带当前 SSH 会话的终端输出、监控指标与服务器身份，
//! 并约定命令执行协议（```run 块）。
//! 实际采集由应用层（desktop）实现：scrollback / 指标摘要 / 会话元信息在
//! `ai_chat_send` 中统一注入（单一可信来源），前端只传 sessionId + 开关。

/// 运维上下文（Agent Chat 模式用）
///
/// 前端经 Tauri IPC 传入（camelCase 字段），故 rename_all。
/// 新增字段务必带 `#[serde(default)]`，避免旧前端参数反序列化失败。
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Context {
    /// 关联的 SSH 会话 ID（None 表示纯通用问答）
    pub session_id: Option<String>,
    /// 终端最近 N 行输出（应用层采集后传入）
    pub terminal_output: Option<String>,
    /// 监控指标摘要（应用层采集后传入，如 "CPU 82.5% / MEM 71.2% / load1 3.4"）
    #[serde(default)]
    pub metrics_summary: Option<String>,
    /// 目标服务器身份（应用层注入，如 "root@192.168.1.10:22"）
    #[serde(default)]
    pub server_info: Option<String>,
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
            server_info: None,
            include_context: false,
        }
    }

    /// 创建"附带上下文"的 Context
    pub fn with_terminal(session_id: String, terminal_output: String) -> Self {
        Self {
            session_id: Some(session_id),
            terminal_output: Some(terminal_output),
            metrics_summary: None,
            server_info: None,
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
        if let Some(s) = self.server_info.as_ref() {
            prompt.push_str(&format!("\n\n## 目标服务器\n`{}`", s));
            prompt.push_str(
                "\n\n## 命令执行协议\n你可以让用户在这台服务器上执行命令来获取信息：在回复中输出一个 ```run 代码块，内容为一条非交互式 shell 命令（每次回复最多一个 run 块），用户确认后会执行并把输出回传给你，你再继续分析。要求：\n- 命令必须是只读或安全的诊断类命令，优先使用简洁精准的命令\n- 执行有状态或破坏性的操作（重启、删除、修改配置等）前，必须先用文字说明影响并提醒用户\n- 禁止输出需要交互输入的命令（如需要输密码的 sudo）；只读信息尽量自行用 run 块获取，不要让用户手抄命令",
            );
            has_any = true;
        }
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
