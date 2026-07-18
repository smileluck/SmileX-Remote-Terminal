//! # AI 助手命令组
//!
//! 对接前端 Chat 面板：发送（流式）、中断、清空、配置更新。

use std::sync::Arc;

use tauri::{AppHandle, Emitter, State};

use crate::events::{AiDonePayload, AiTokenPayload};
use crate::AppState;
use ai_core::provider::context::Context;
use ai_core::LlmProviderConfig;

/// 发送消息（流式响应通过 `ai_token` 事件推送，结束发 `ai_done`）
#[tauri::command]
pub async fn ai_chat_send(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    message: String,
    ctx: Context,
) -> Result<(), String> {
    let provider = state.chat_provider.clone();

    // token 回调：emit `ai_token` 事件
    let app_clone = app.clone();
    let session_id_clone = session_id.clone();
    let on_token = Arc::new(move |token: String| {
        let _ = app_clone.emit(
            "ai_token",
            AiTokenPayload {
                session_id: session_id_clone.clone(),
                token,
            },
        );
    }) as Arc<dyn Fn(String) + Send + Sync>;

    // 调用 ChatProvider
    let result = provider.send(&message, &ctx, on_token).await;

    // 无论成功/失败/取消，都发 ai_done 让前端停止 loading
    let _ = app.emit(
        "ai_done",
        AiDonePayload {
            session_id,
            success: result.is_ok(),
            error: result.as_ref().err().map(|e| format!("{e}")),
        },
    );

    result.map_err(|e| format!("{e}"))?;
    Ok(())
}

/// 中断当前生成
#[tauri::command]
pub async fn ai_chat_abort(state: State<'_, AppState>) -> Result<(), String> {
    state
        .chat_provider
        .abort()
        .await
        .map_err(|e| format!("{e}"))?;
    Ok(())
}

/// 清空对话历史
#[tauri::command]
pub async fn ai_chat_clear(state: State<'_, AppState>) -> Result<(), String> {
    state
        .chat_provider
        .clear()
        .await
        .map_err(|e| format!("{e}"))?;
    Ok(())
}

/// 更新 LLM Provider 配置
#[tauri::command]
pub async fn ai_update_config(
    state: State<'_, AppState>,
    config: LlmProviderConfig,
) -> Result<(), String> {
    state.chat_provider.update_config(config).await;
    Ok(())
}
