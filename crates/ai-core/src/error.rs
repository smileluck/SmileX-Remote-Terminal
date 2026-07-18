//! # 统一错误类型

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    /// LLM API 调用失败
    #[error("LLM 调用失败: {0}")]
    LlmApi(String),

    /// LLM 配置错误（缺 API Key 等）
    #[error("LLM 配置错误: {0}")]
    LlmConfig(String),

    /// 流式响应解析失败
    #[error("流式响应解析失败: {0}")]
    StreamParse(String),

    /// 对话历史超限
    #[error("对话历史超限")]
    HistoryOverflow,

    /// 上下文采集失败
    #[error("上下文采集失败: {0}")]
    Context(String),

    /// 操作被取消（用户中断）
    #[error("操作已取消")]
    Cancelled,

    /// Work 模式未实现
    #[error("Work 模式未实现（预留接口）")]
    WorkNotImplemented,

    /// 其他
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
