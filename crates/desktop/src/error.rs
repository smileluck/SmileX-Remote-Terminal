//! # 结构化应用错误
//!
//! Tauri command 统一错误类型，序列化为 `{ "code": "...", "message": "..." }`，
//! 前端 [`services/invoke`](../../../app/src/services/invoke.ts) 统一解析展示。
//!
//! - `code`：错误域（session/storage/monitor/ai/desktop/llm/internal），
//!   前端可按域定制提示或处理
//! - `message`：面向用户的中文错误信息
//!
//! 已有大量 `.map_err(|e| format!(...))?` 写法通过 [`From<String>`] 兜底转换
//! （code = internal），新代码建议显式使用域构造器。

use serde::ser::SerializeStruct;

/// 错误码（错误域）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    Session,
    Storage,
    Monitor,
    Ai,
    Desktop,
    Llm,
    Internal,
}

impl ErrorCode {
    fn as_str(self) -> &'static str {
        match self {
            ErrorCode::Session => "session",
            ErrorCode::Storage => "storage",
            ErrorCode::Monitor => "monitor",
            ErrorCode::Ai => "ai",
            ErrorCode::Desktop => "desktop",
            ErrorCode::Llm => "llm",
            ErrorCode::Internal => "internal",
        }
    }
}

/// 应用统一错误
#[derive(Debug, thiserror::Error)]
pub struct AppError {
    code: ErrorCode,
    message: String,
}

impl AppError {
    /// 按错误域构造
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn session(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Session, message)
    }
    pub fn storage(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Storage, message)
    }
    pub fn monitor(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Monitor, message)
    }
    pub fn ai(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Ai, message)
    }
    pub fn desktop(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Desktop, message)
    }
    pub fn llm(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Llm, message)
    }
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

/// 兼容既有 `.map_err(|e| format!(...))?` 链路（code = internal）
impl From<String> for AppError {
    fn from(message: String) -> Self {
        Self::new(ErrorCode::Internal, message)
    }
}

impl From<&str> for AppError {
    fn from(message: &str) -> Self {
        Self::new(ErrorCode::Internal, message)
    }
}

/// 序列化为 `{ "code": "...", "message": "..." }`
impl serde::Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut s = serializer.serialize_struct("AppError", 2)?;
        s.serialize_field("code", self.code.as_str())?;
        s.serialize_field("message", &self.message)?;
        s.end()
    }
}

/// 命令返回类型别名
pub type AppResult<T> = Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialize_shape() {
        let e = AppError::session("会话不存在");
        let json = serde_json::to_value(&e).unwrap();
        assert_eq!(json["code"], "session");
        assert_eq!(json["message"], "会话不存在");
    }

    #[test]
    fn from_string_internal() {
        let e: AppError = "普通字符串".into();
        assert_eq!(serde_json::to_value(&e).unwrap()["code"], "internal");
    }
}
