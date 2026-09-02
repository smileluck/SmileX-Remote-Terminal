//! # AI 助手命令组
//!
//! 对接前端 Chat 面板：发送（流式）、中断、清空、配置更新。
//!
//! ## 配置持久化（阶段 5）
//! - 非敏感字段（provider/model/base_url/stream）：序列化为 JSON 存 SQLite `app_config` 表
//! - 敏感字段（api_key）：单独存 OS Keyring
//!
//! 启动时 `main.rs` 调用 [`load_and_apply_llm_config`] 加载持久化配置。

use std::sync::Arc;

use tauri::{AppHandle, Emitter, State};

use crate::events::{AiDonePayload, AiTokenPayload};
use crate::storage::keyring;
use crate::AppState;
use ai_core::provider::context::Context;
// 引入 AgentProvider trait 才能调用 Arc<ChatProvider> 上的 send/abort/clear
use ai_core::AgentProvider;
use ai_core::LlmProviderConfig;

/// SQLite `app_config` 中 LLM 配置的键
const CONFIG_KEY: &str = "llm_config";
/// Keyring 中 API Key 的键
const SECRET_KEY: &str = "ai:api_key";

/// 运行时长格式化（如 "3天4小时"）
fn format_uptime(s: u64) -> String {
    let days = s / 86400;
    let hours = (s % 86400) / 3600;
    if days > 0 {
        format!("{days}天{hours}小时")
    } else if hours > 0 {
        format!("{hours}小时{}分", (s % 3600) / 60)
    } else {
        format!("{}分", s / 60)
    }
}

/// 发送消息（流式响应通过 `ai_token` 事件推送，结束发 `ai_done`）
#[tauri::command]
pub async fn ai_chat_send(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    message: String,
    ctx: Context,
) -> Result<(), crate::error::AppError> {
    let provider = state.chat_provider.clone();

    // 上下文注入：前端只传 sessionId + include_context 标志，
    // 终端输出、监控摘要与服务器身份由后端在此填充（单一可信来源）
    let mut ctx = ctx;
    if ctx.include_context {
        if let Some(sid) = ctx.session_id.clone() {
            // 目标服务器身份（user@host:port，Agent 的命令执行目标）
            if let Some(info) = state.session_meta.lock().await.get(&sid).cloned() {
                ctx.server_info = Some(info);
            }
            // 终端回滚缓冲（约最近 200 行）
            let scrollback = state
                .terminal_scrollback
                .lock()
                .await
                .get(&sid)
                .cloned();
            if let Some(sb) = scrollback {
                let text = sb.lock().await.clone();
                if !text.trim().is_empty() {
                    ctx.terminal_output = Some(text);
                }
            }
            // 最近监控采样摘要
            if let Some(m) = state.monitor_sampler.last_metrics(&sid).await {
                let disk = m
                    .disks
                    .first()
                    .map(|d| format!(" / 磁盘{} {:.1}%", d.mount, d.used_percent))
                    .unwrap_or_default();
                ctx.metrics_summary = Some(format!(
                    "CPU {:.1}% / 内存 {:.1}% ({:.1}G/{:.1}G) / 负载1m {:.2}{} / 网络 ↓{:.0}KB/s ↑{:.0}KB/s / 延迟 {}ms / 运行 {}",
                    m.cpu_percent.unwrap_or(0.0),
                    m.mem_percent,
                    m.mem_used_bytes as f64 / 1024.0 / 1024.0 / 1024.0,
                    m.mem_total_bytes as f64 / 1024.0 / 1024.0 / 1024.0,
                    m.load1,
                    disk,
                    m.net_rx_bps / 1024.0,
                    m.net_tx_bps / 1024.0,
                    m.latency_ms.unwrap_or(0),
                    format_uptime(m.uptime_s),
                ));
            }
        }
    }

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
            error: result.as_ref().err().map(|e| e.to_string()),
        },
    );

    result.map_err(|e| crate::error::AppError::ai(format!("{e}")))?;
    Ok(())
}

/// 中断当前生成
#[tauri::command]
pub async fn ai_chat_abort(state: State<'_, AppState>) -> Result<(), crate::error::AppError> {
    state
        .chat_provider
        .abort()
        .await
        .map_err(|e| crate::error::AppError::ai(format!("{e}")))?;
    Ok(())
}

/// 清空对话历史
#[tauri::command]
pub async fn ai_chat_clear(state: State<'_, AppState>) -> Result<(), crate::error::AppError> {
    state
        .chat_provider
        .clear()
        .await
        .map_err(|e| crate::error::AppError::ai(format!("{e}")))?;
    Ok(())
}

/// 更新 LLM Provider 配置（仅内存，不持久化）
///
/// 前端调用此命令临时应用配置。持久化请用 [`ai_config_save`]。
#[tauri::command]
pub async fn ai_update_config(
    state: State<'_, AppState>,
    config: LlmProviderConfig,
) -> Result<(), crate::error::AppError> {
    state.chat_provider.update_config(config).await;
    Ok(())
}

/// 读取持久化的 LLM 配置（不含 API Key）
///
/// 返回 `Ok(Some(config))` / `Ok(None)`（未配置过）。
#[tauri::command]
pub async fn ai_config_get(
    state: State<'_, AppState>,
) -> Result<Option<LlmProviderConfig>, crate::error::AppError> {
    let json = state
        .storage
        .get_config(CONFIG_KEY)
        .await
        .map_err(|e| crate::error::AppError::ai(format!("读取 LLM 配置失败: {e}")))?;

    if let Some(json) = json {
        let mut config: LlmProviderConfig = serde_json::from_str(&json)
            .map_err(|e| crate::error::AppError::ai(format!("解析 LLM 配置 JSON 失败: {e}")))?;
        // 出于安全考虑，持久化的值不通过此命令返回 API Key
        config.api_key = None;
        Ok(Some(config))
    } else {
        Ok(None)
    }
}

/// 持久化 LLM 配置
///
/// - 非敏感字段（provider/model/base_url/stream）→ SQLite `app_config`
/// - API Key（若 `api_key` 为 Some 且非空）→ OS Keyring
/// - `api_key == Some("")` 视为清除 Keyring
/// - `api_key == None` 表示保持现状不变
///
/// 持久化后同步应用到当前 ChatProvider（含从 Keyring 合并的 api_key）。
#[tauri::command]
pub async fn ai_config_save(
    state: State<'_, AppState>,
    mut config: LlmProviderConfig,
) -> Result<(), crate::error::AppError> {
    let now = chrono::Utc::now().timestamp();

    // 1) 处理 API Key（Some/non-empty 写入；Some/empty 清除；None 保持）
    match config.api_key.take() {
        Some(k) if !k.is_empty() => {
            keyring::set_credential(SECRET_KEY, &k)
                .map_err(|e| crate::error::AppError::ai(format!("写入 API Key 到 Keyring 失败: {e}")))?;
            // 重新合并以便后续 apply 使用
            config.api_key = Some(k);
        }
        Some(_) => {
            // 空字符串 → 清除 Keyring
            let _ = keyring::delete_credential(SECRET_KEY);
        }
        None => {
            // 保持现状：从 Keyring 读回合并
            if let Ok(Some(k)) = keyring::get_credential(SECRET_KEY) {
                config.api_key = Some(k);
            }
        }
    }

    // 2) 序列化非敏感字段存 SQLite（api_key 已 take 或重新填充，序列化时需排除）
    //    做法：临时置 None 后序列化，再恢复
    let api_key_for_apply = config.api_key.clone();
    let mut for_storage = config.clone();
    for_storage.api_key = None;
    let json = serde_json::to_string(&for_storage)
        .map_err(|e| crate::error::AppError::ai(format!("序列化 LLM 配置失败: {e}")))?;
    state
        .storage
        .set_config(CONFIG_KEY, &json, now)
        .await
        .map_err(|e| crate::error::AppError::ai(format!("写入 LLM 配置到 SQLite 失败: {e}")))?;

    // 3) 应用到当前 ChatProvider（含 api_key）
    let mut applied = config;
    applied.api_key = api_key_for_apply;
    state.chat_provider.update_config(applied).await;

    tracing::info!("LLM 配置已持久化并应用");
    Ok(())
}

/// 读取 API Key（设置页编辑时回填用）
///
/// 仅在用户主动打开设置页时调用，避免 API Key 常驻前端内存。
#[tauri::command]
pub async fn ai_secret_get(_state: State<'_, AppState>) -> Result<Option<String>, crate::error::AppError> {
    keyring::get_credential(SECRET_KEY).map_err(|e| crate::error::AppError::ai(format!("读取 API Key 失败: {e}")))
}

/// 从持久化加载 LLM 配置并应用到 ChatProvider（启动时调用）
///
/// 此函数不是 Tauri command，由 `main.rs` setup 阶段直接调用。
///
/// 流程：
/// 1. 从 SQLite 读取非敏感配置 JSON
/// 2. 从 Keyring 读取 API Key
/// 3. 合并后应用到 ChatProvider
///
/// 任何一步失败只记日志不中断启动（让用户能在设置页重新配置）。
pub async fn load_and_apply_llm_config(state: &AppState) {
    // 1) 读 SQLite
    let json = match state.storage.get_config(CONFIG_KEY).await {
        Ok(Some(v)) => v,
        Ok(None) => {
            tracing::info!("未找到持久化 LLM 配置，使用默认配置（需用户在设置页配置）");
            return;
        }
        Err(e) => {
            tracing::warn!(error = %e, "读取 LLM 配置失败，跳过自动应用");
            return;
        }
    };

    let mut config: LlmProviderConfig = match serde_json::from_str(&json) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(error = %e, "解析 LLM 配置 JSON 失败，跳过自动应用");
            return;
        }
    };

    // 2) 读 Keyring 合并 api_key
    match keyring::get_credential(SECRET_KEY) {
        Ok(Some(k)) => {
            config.api_key = Some(k);
        }
        Ok(None) => {
            tracing::info!("Keyring 中无 API Key（provider 可能是 ollama 等无需 key 的类型）");
        }
        Err(e) => {
            tracing::warn!(error = %e, "读取 API Key 失败，跳过自动应用");
            return;
        }
    }

    // 3) 应用
    state.chat_provider.update_config(config).await;
    tracing::info!("LLM 配置已从持久化加载并应用");
}
