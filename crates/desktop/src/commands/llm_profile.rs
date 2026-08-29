//! # LLM 配置档案命令组
//!
//! 多 LLM 配置档案管理：保存/列出/删除/激活/连通性测试。
//!
//! ## 数据流
//! - 非敏感字段（name/provider/model/base_url/stream/is_active）→ SQLite `llm_profiles` 表
//! - API Key → OS Keyring（key: `llm_profile:{id}:api_key`）
//!
//! ## 激活机制
//! 同一时间仅一个档案为 active。切换激活会：
//! 1. 清除其他档案的 `is_active`
//! 2. 设置目标档案 `is_active = 1`
//! 3. 立即应用到 `ChatProvider`（读取 Keyring 合并 api_key 后调用 `update_config`）
//!
//! ## 连通性测试
//! 通过 [`ai_core::LlmClient`] 临时构造客户端，发送 "ping" 消息，
//! 使用 `tokio::time::timeout` 等待首个 token 到达即视为成功。
//! 避免完整生成响应浪费时间。

use std::sync::Arc;
use std::time::Duration;

use tauri::State;
use tokio::sync::oneshot;

use crate::storage::keyring;
use crate::storage::sqlite::LlmProfile;
use crate::AppState;
use ai_core::provider::history::{Message, Role};
use ai_core::provider::llm::{LlmProvider, create_client};
use ai_core::LlmProviderConfig;

/// Keyring key 前缀（与 profile id 拼接）
const KEYRING_PREFIX: &str = "llm_profile:";
/// 旧版单 key 配置的 SQLite 键（用于自动迁移）
pub const LEGACY_CONFIG_KEY: &str = "llm_config";
/// 旧版单 key API Key 的 Keyring 键
pub const LEGACY_SECRET_KEY: &str = "ai:api_key";

/// 构造 Keyring key
fn keyring_key(profile_id: &str) -> String {
    format!("{KEYRING_PREFIX}{profile_id}:api_key")
}

/// 把字符串 provider 名转为 `LlmProvider` 枚举
///
/// 未知值返回错误（避免静默使用错误的 Provider）。
fn parse_provider(s: &str) -> Result<LlmProvider, crate::error::AppError> {
    match s.to_lowercase().as_str() {
        "openai" => Ok(LlmProvider::OpenAi),
        "claude" => Ok(LlmProvider::Claude),
        "ollama" => Ok(LlmProvider::Ollama),
        other => Err(crate::error::AppError::llm(format!("未知的 Provider 类型: {other}"))),
    }
}

/// 把 `LlmProfile` + `api_key` 构造为 `LlmProviderConfig`（应用到 ChatProvider）
fn build_config(profile: &LlmProfile, api_key: Option<String>) -> Result<LlmProviderConfig, crate::error::AppError> {
    Ok(LlmProviderConfig {
        provider: parse_provider(&profile.provider)?,
        model: profile.model.clone(),
        base_url: profile.base_url.clone(),
        api_key,
        stream: profile.stream,
    })
}

/// 保存 LLM 配置档案
///
/// - 非敏感字段 → SQLite
/// - API Key 三态语义：
///   - `Some(非空)` → 写入 Keyring
///   - `Some(空)` → 清除 Keyring
///   - `None` → 保持现状（从 Keyring 读回）
///
/// 若该档案当前为 active，保存后自动应用到 ChatProvider。
#[tauri::command]
pub async fn llm_profile_save(
    state: State<'_, AppState>,
    mut profile: LlmProfile,
    api_key: Option<String>,
) -> Result<LlmProfile, crate::error::AppError> {
    let now = chrono::Utc::now().timestamp();
    if profile.created_at == 0 {
        profile.created_at = now;
    }
    profile.updated_at = now;

    // 处理 API Key（三态）
    let resolved_api_key: Option<String> = match api_key {
        Some(k) if !k.is_empty() => {
            keyring::set_credential(&keyring_key(&profile.id), &k)
                .map_err(|e| crate::error::AppError::llm(format!("写入 API Key 失败: {e}")))?;
            Some(k)
        }
        Some(_) => {
            // 空字符串 → 清除
            let _ = keyring::delete_credential(&keyring_key(&profile.id));
            None
        }
        None => {
            // 保持现状：读回
            keyring::get_credential(&keyring_key(&profile.id))
                .map_err(|e| crate::error::AppError::llm(format!("读取 API Key 失败: {e}")))?
        }
    };

    let is_active = profile.is_active;

    // 持久化到 SQLite
    state
        .storage
        .save_llm_profile(&profile)
        .await
        .map_err(|e| crate::error::AppError::llm(format!("保存 LLM 配置失败: {e}")))?;

    // 若是 active，同步应用到 ChatProvider
    if is_active {
        let config = build_config(&profile, resolved_api_key)?;
        state.chat_provider.update_config(config).await;
        tracing::info!(profile_id = %profile.id, "active LLM 配置已更新并应用");
    }

    tracing::info!(profile_id = %profile.id, name = %profile.name, "LLM 配置已保存");
    Ok(profile)
}

/// 列出所有 LLM 配置档案（不含 API Key）
#[tauri::command]
pub async fn llm_profile_list(
    state: State<'_, AppState>,
) -> Result<Vec<LlmProfile>, crate::error::AppError> {
    state
        .storage
        .list_llm_profiles()
        .await
        .map_err(|e| crate::error::AppError::llm(format!("列出 LLM 配置失败: {e}")))
}

/// 按 id 获取单个 LLM 配置档案（不含 API Key）
#[tauri::command]
pub async fn llm_profile_get(
    state: State<'_, AppState>,
    id: String,
) -> Result<Option<LlmProfile>, crate::error::AppError> {
    state
        .storage
        .get_llm_profile(&id)
        .await
        .map_err(|e| crate::error::AppError::llm(format!("获取 LLM 配置失败: {e}")))
}

/// 读取指定档案的 API Key（编辑时回填用）
#[tauri::command]
pub async fn llm_profile_get_api_key(
    _state: State<'_, AppState>,
    id: String,
) -> Result<Option<String>, crate::error::AppError> {
    keyring::get_credential(&keyring_key(&id)).map_err(|e| crate::error::AppError::llm(format!("读取 API Key 失败: {e}")))
}

/// 删除 LLM 配置档案（同时清理 SQLite + Keyring）
#[tauri::command]
pub async fn llm_profile_delete(
    state: State<'_, AppState>,
    id: String,
) -> Result<bool, crate::error::AppError> {
    // 先删 SQLite
    let deleted = state
        .storage
        .delete_llm_profile(&id)
        .await
        .map_err(|e| crate::error::AppError::llm(format!("删除 LLM 配置失败: {e}")))?;

    // 再清 Keyring（即使 SQLite 删除失败也尝试清理）
    let _ = keyring::delete_credential(&keyring_key(&id));

    if deleted {
        tracing::info!(profile_id = %id, "LLM 配置已删除");
    }
    Ok(deleted)
}

/// 设置激活的 LLM 配置档案
///
/// 流程：
/// 1. 排他性地设置 `is_active = 1`
/// 2. 从 Keyring 读取 API Key
/// 3. 构造 `LlmProviderConfig` 并应用到 `ChatProvider`
#[tauri::command]
pub async fn llm_profile_set_active(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), crate::error::AppError> {
    // 1) 设置 active 标记
    state
        .storage
        .set_active_llm_profile(&id)
        .await
        .map_err(|e| crate::error::AppError::llm(format!("设置激活 LLM 配置失败: {e}")))?;

    // 2) 读取完整 profile + api_key
    let profile = state
        .storage
        .get_llm_profile(&id)
        .await
        .map_err(|e| crate::error::AppError::llm(format!("读取激活配置失败: {e}")))?
        .ok_or_else(|| format!("配置 {id} 不存在"))?;

    let api_key = keyring::get_credential(&keyring_key(&id))
        .map_err(|e| crate::error::AppError::llm(format!("读取 API Key 失败: {e}")))?;

    // 3) 应用
    let config = build_config(&profile, api_key)?;
    state.chat_provider.update_config(config).await;

    tracing::info!(profile_id = %id, "已切换激活 LLM 配置");
    Ok(())
}

/// 获取当前激活的 LLM 配置档案（不含 API Key）
#[tauri::command]
pub async fn llm_profile_get_active(
    state: State<'_, AppState>,
) -> Result<Option<LlmProfile>, crate::error::AppError> {
    state
        .storage
        .get_active_llm_profile()
        .await
        .map_err(|e| crate::error::AppError::llm(format!("读取激活 LLM 配置失败: {e}")))
}

/// 连通性测试
///
/// 流程：
/// 1. 读取 profile + api_key
/// 2. 构造 `LlmProviderConfig`
/// 3. 通过 `create_client` 创建临时客户端
/// 4. 发送 "ping" 消息，等待首个 token 到达（10s 超时）
/// 5. 收到首个 token → 成功；超时/错误 → 失败
///
/// **注**：不会干扰当前 ChatProvider 状态。
#[tauri::command]
pub async fn llm_profile_test(
    state: State<'_, AppState>,
    id: String,
) -> Result<String, crate::error::AppError> {
    // 1) 读取 profile
    let profile = state
        .storage
        .get_llm_profile(&id)
        .await
        .map_err(|e| crate::error::AppError::llm(format!("读取配置失败: {e}")))?
        .ok_or_else(|| format!("配置 {id} 不存在"))?;

    let api_key = keyring::get_credential(&keyring_key(&id))
        .map_err(|e| crate::error::AppError::llm(format!("读取 API Key 失败: {e}")))?;

    // 2) 构造 config + 临时 client
    let config = build_config(&profile, api_key)?;
    let client = create_client(&config);

    // 3) 发送测试消息：用 oneshot 等待首个 token
    let (tx, rx) = oneshot::channel::<String>();
    let tx = Arc::new(tokio::sync::Mutex::new(Some(tx)));
    let on_token = Arc::new(move |token: String| {
        // 首个 token 到达：取出 sender 发送（已发送则忽略）
        if let Some(sender) = tx.try_lock().ok().and_then(|mut g| g.take()) {
            let _ = sender.send(token);
        }
    }) as Arc<dyn Fn(String) + Send + Sync>;

    let messages = vec![Message {
        role: Role::User,
        content: "ping".into(),
    }];

    // 4) tokio::spawn 后台调用 chat_stream（不等其完成，只等首个 token）
    let client_clone = client.clone();
    let messages_clone = messages.clone();
    let on_token_clone = on_token.clone();
    let chat_task = tokio::spawn(async move {
        client_clone
            .chat_stream(&messages_clone, on_token_clone)
            .await
    });

    // 5) 等待首个 token（10s 超时）
    match tokio::time::timeout(Duration::from_secs(10), rx).await {
        Ok(Ok(first_token)) => {
            // 成功：中止后台任务（不需要完整响应）
            chat_task.abort();
            Ok(first_token)
        }
        Ok(Err(_)) => {
            // sender 被 drop（理论上不会发生）
            Err("测试失败：未能接收到响应".into())
        }
        Err(_) => {
            // 超时
            chat_task.abort();
            Err("连接超时（10s 未收到响应），请检查 Base URL / API Key".into())
        }
    }
}

/// 从持久化加载并应用激活的 LLM 配置（启动时调用）
///
/// 若无激活档案但存在旧版单 key 配置（`llm_config`），自动迁移为 `默认` profile。
///
/// 此函数不是 Tauri command，由 `main.rs` setup 调用。
/// 失败只记日志不中断启动。
pub async fn load_and_apply_active_profile(state: &AppState) {
    // 1) 尝试读取激活档案
    match state.storage.get_active_llm_profile().await {
        Ok(Some(profile)) => {
            // 读取 API Key 合并应用
            let api_key = match keyring::get_credential(&keyring_key(&profile.id)) {
                Ok(k) => k,
                Err(e) => {
                    tracing::warn!(error = %e, "读取激活档案 API Key 失败");
                    None
                }
            };

            match build_config(&profile, api_key) {
                Ok(config) => {
                    state.chat_provider.update_config(config).await;
                    tracing::info!(profile_id = %profile.id, "已加载激活 LLM 配置");
                }
                Err(e) => {
                    tracing::warn!(error = %e, "构造激活 LlmProviderConfig 失败");
                }
            }
            return;
        }
        Ok(None) => {
            tracing::info!("无激活 LLM 档案，尝试迁移旧版配置...");
        }
        Err(e) => {
            tracing::warn!(error = %e, "读取激活 LLM 档案失败，尝试迁移旧版配置...");
        }
    }

    // 2) 迁移旧版单 key 配置
    migrate_legacy_config(state).await;
}

/// 迁移旧版单 key `llm_config` 到新的多档案格式
///
/// 仅在首次升级时触发（新表为空 + 旧 key 存在）。
/// 迁移成功后删除旧 key（避免重复迁移）。
async fn migrate_legacy_config(state: &AppState) {
    let json = match state.storage.get_config(LEGACY_CONFIG_KEY).await {
        Ok(Some(v)) => v,
        Ok(None) => {
            tracing::info!("无旧版 LLM 配置，跳过迁移");
            return;
        }
        Err(e) => {
            tracing::warn!(error = %e, "读取旧版 LLM 配置失败");
            return;
        }
    };

    let mut config: LlmProviderConfig = match serde_json::from_str(&json) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(error = %e, "解析旧版 LLM 配置 JSON 失败");
            return;
        }
    };

    // 从旧 Keyring 读 API Key
    let legacy_api_key = keyring::get_credential(LEGACY_SECRET_KEY).ok().flatten();
    config.api_key = legacy_api_key.clone();

    // 创建默认 profile
    let now = chrono::Utc::now().timestamp();
    let profile_id = uuid::Uuid::new_v4().to_string();
    let profile = LlmProfile {
        id: profile_id.clone(),
        name: "默认".into(),
        provider: format!("{:?}", config.provider).to_lowercase(),
        model: config.model.clone(),
        base_url: config.base_url.clone(),
        stream: config.stream,
        is_active: true,
        created_at: now,
        updated_at: now,
    };

    // 写入新表
    if let Err(e) = state.storage.save_llm_profile(&profile).await {
        tracing::warn!(error = %e, "迁移：写入新 profile 失败");
        return;
    }

    // 迁移 API Key 到新 Keyring key
    if let Some(k) = legacy_api_key {
        let _ = keyring::set_credential(&keyring_key(&profile_id), &k);
        let _ = keyring::delete_credential(LEGACY_SECRET_KEY);
    }

    // 应用
    state.chat_provider.update_config(config).await;

    // 删除旧 SQLite key（避免重复迁移）
    let _ = state.storage.set_config(LEGACY_CONFIG_KEY, "", now).await;

    tracing::info!(profile_id = %profile_id, "旧版 LLM 配置已迁移为默认档案");
}
