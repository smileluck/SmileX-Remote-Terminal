//! # Agent 助手会话命令组
//!
//! Chat 面板多会话 Tab：会话（列表/新建/重命名/删除）与
//! 消息（查询/追加/清空）持久化到 SQLite。

use tauri::State;

use crate::error::AppError;
use crate::storage::sqlite::{AgentChat, AgentChatMessage};
use crate::AppState;

/// 全部 Agent 会话（最近更新在前）
#[tauri::command]
pub async fn agent_chat_list(state: State<'_, AppState>) -> Result<Vec<AgentChat>, AppError> {
    state
        .storage
        .list_chats()
        .await
        .map_err(|e| AppError::storage(format!("查询 Agent 会话失败: {e}")))
}

/// 新建 Agent 会话
#[tauri::command]
pub async fn agent_chat_create(
    state: State<'_, AppState>,
    chat: AgentChat,
) -> Result<(), AppError> {
    state
        .storage
        .create_chat(&chat)
        .await
        .map_err(|e| AppError::storage(format!("创建 Agent 会话失败: {e}")))
}

/// 重命名 Agent 会话
#[tauri::command]
pub async fn agent_chat_rename(
    state: State<'_, AppState>,
    chat_id: String,
    title: String,
) -> Result<(), AppError> {
    state
        .storage
        .rename_chat(&chat_id, &title)
        .await
        .map_err(|e| AppError::storage(format!("重命名 Agent 会话失败: {e}")))
}

/// 删除 Agent 会话（级联删消息，返回是否存在）
#[tauri::command]
pub async fn agent_chat_delete(
    state: State<'_, AppState>,
    chat_id: String,
) -> Result<bool, AppError> {
    state
        .storage
        .delete_chat(&chat_id)
        .await
        .map_err(|e| AppError::storage(format!("删除 Agent 会话失败: {e}")))
}

/// 某会话全部消息（旧→新）
#[tauri::command]
pub async fn agent_chat_messages(
    state: State<'_, AppState>,
    chat_id: String,
) -> Result<Vec<AgentChatMessage>, AppError> {
    state
        .storage
        .list_chat_messages(&chat_id)
        .await
        .map_err(|e| AppError::storage(format!("查询 Agent 会话消息失败: {e}")))
}

/// 追加一条会话消息（touch 会话 updated_at；未命名会话自动生成标题）
///
/// 返回更新后的会话，供前端同步列表排序与标题。
#[tauri::command]
pub async fn agent_chat_message_append(
    state: State<'_, AppState>,
    message: AgentChatMessage,
) -> Result<AgentChat, AppError> {
    let ts = chrono::Utc::now().timestamp();
    state
        .storage
        .append_chat_message(&message, ts)
        .await
        .map_err(|e| AppError::storage(format!("追加 Agent 会话消息失败: {e}")))
}

/// 清空某会话的消息（保留会话本身）
#[tauri::command]
pub async fn agent_chat_clear_messages(
    state: State<'_, AppState>,
    chat_id: String,
) -> Result<(), AppError> {
    state
        .storage
        .clear_chat_messages(&chat_id)
        .await
        .map_err(|e| AppError::storage(format!("清空 Agent 会话消息失败: {e}")))
}
