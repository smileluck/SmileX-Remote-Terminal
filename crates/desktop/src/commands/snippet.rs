//! # 命令片段 & 历史命令组
//!
//! 命令片段：用户收藏的常用命令（CRUD）。
//! 命令历史：终端回车行自动记录（追加/查询/清空）。

use tauri::State;

use crate::error::AppError;
use crate::storage::sqlite::{CommandHistory, CommandSnippet, FavoriteDir};
use crate::AppState;

/// 全部命令片段（新→旧）
#[tauri::command]
pub async fn snippet_list(
    state: State<'_, AppState>,
) -> Result<Vec<CommandSnippet>, AppError> {
    state
        .storage
        .list_snippets()
        .await
        .map_err(|e| AppError::storage(format!("查询命令片段失败: {e}")))
}

/// 保存/更新命令片段
#[tauri::command]
pub async fn snippet_save(
    state: State<'_, AppState>,
    snippet: CommandSnippet,
) -> Result<(), AppError> {
    state
        .storage
        .save_snippet(&snippet)
        .await
        .map_err(|e| AppError::storage(format!("保存命令片段失败: {e}")))
}

/// 删除命令片段
#[tauri::command]
pub async fn snippet_delete(state: State<'_, AppState>, id: String) -> Result<bool, AppError> {
    state
        .storage
        .delete_snippet(&id)
        .await
        .map_err(|e| AppError::storage(format!("删除命令片段失败: {e}")))
}

/// 批量重排命令片段（分组/排序拖拽落库）
#[tauri::command]
pub async fn snippet_reorder(
    state: State<'_, AppState>,
    snippets: Vec<CommandSnippet>,
) -> Result<(), AppError> {
    state
        .storage
        .reorder_snippets(&snippets)
        .await
        .map_err(|e| AppError::storage(format!("重排命令片段失败: {e}")))
}

/// 记录一条命令历史（终端回车行）
#[tauri::command]
pub async fn history_add(
    state: State<'_, AppState>,
    session_id: String,
    command: String,
) -> Result<(), AppError> {
    if command.trim().is_empty() {
        return Ok(());
    }
    let ts = chrono::Utc::now().timestamp();
    state
        .storage
        .add_history(&session_id, &command, ts)
        .await
        .map_err(|e| AppError::storage(format!("记录命令历史失败: {e}")))
}

/// 最近命令历史（新→旧）
#[tauri::command]
pub async fn history_list(
    state: State<'_, AppState>,
    limit: Option<u32>,
) -> Result<Vec<CommandHistory>, AppError> {
    state
        .storage
        .list_history(limit.unwrap_or(100))
        .await
        .map_err(|e| AppError::storage(format!("查询命令历史失败: {e}")))
}

/// 清空命令历史
#[tauri::command]
pub async fn history_clear(state: State<'_, AppState>) -> Result<(), AppError> {
    state
        .storage
        .clear_history()
        .await
        .map_err(|e| AppError::storage(format!("清空命令历史失败: {e}")))
}

/// 指定主机档案的常用目录列表
#[tauri::command]
pub async fn favorite_dir_list(
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<Vec<FavoriteDir>, AppError> {
    state
        .storage
        .list_favorite_dirs(&profile_id)
        .await
        .map_err(|e| AppError::storage(format!("查询常用目录失败: {e}")))
}

/// 保存/更新常用目录
#[tauri::command]
pub async fn favorite_dir_save(
    state: State<'_, AppState>,
    dir: FavoriteDir,
) -> Result<(), AppError> {
    state
        .storage
        .save_favorite_dir(&dir)
        .await
        .map_err(|e| AppError::storage(format!("保存常用目录失败: {e}")))
}

/// 删除常用目录
#[tauri::command]
pub async fn favorite_dir_delete(
    state: State<'_, AppState>,
    id: String,
) -> Result<bool, AppError> {
    state
        .storage
        .delete_favorite_dir(&id)
        .await
        .map_err(|e| AppError::storage(format!("删除常用目录失败: {e}")))
}

/// 批量重排常用目录
#[tauri::command]
pub async fn favorite_dir_reorder(
    state: State<'_, AppState>,
    dirs: Vec<FavoriteDir>,
) -> Result<(), AppError> {
    state
        .storage
        .reorder_favorite_dirs(&dirs)
        .await
        .map_err(|e| AppError::storage(format!("重排常用目录失败: {e}")))
}
