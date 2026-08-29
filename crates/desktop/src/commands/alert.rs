//! # 告警规则命令组
//!
//! 告警规则的 CRUD；规则评估在 `monitor::sampler` 采样循环内完成，
//! 触发时经 `alert_fired` 事件推送前端。

use tauri::State;

use crate::error::AppError;
use crate::storage::sqlite::AlertRule;
use crate::AppState;

/// 全部告警规则
#[tauri::command]
pub async fn alert_rule_list(state: State<'_, AppState>) -> Result<Vec<AlertRule>, AppError> {
    state
        .storage
        .list_alert_rules(false)
        .await
        .map_err(|e| AppError::monitor(format!("查询告警规则失败: {e}")))
}

/// 保存/更新告警规则
#[tauri::command]
pub async fn alert_rule_save(state: State<'_, AppState>, rule: AlertRule) -> Result<(), AppError> {
    state
        .storage
        .save_alert_rule(&rule)
        .await
        .map_err(|e| AppError::monitor(format!("保存告警规则失败: {e}")))
}

/// 删除告警规则
#[tauri::command]
pub async fn alert_rule_delete(state: State<'_, AppState>, id: String) -> Result<bool, AppError> {
    state
        .storage
        .delete_alert_rule(&id)
        .await
        .map_err(|e| AppError::monitor(format!("删除告警规则失败: {e}")))
}
