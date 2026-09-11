//! # SmileX Remote Terminal 桌面应用入口
//!
//! Tauri 主进程入口，负责：
//! 1. 初始化日志 / SQLite 存储
//! 2. 注册 Tauri commands
//! 3. 拉起应用窗口

#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

use std::path::PathBuf;

use smilex_desktop::{commands, AppState};
// 引入 Manager trait 以便调用 app_handle / path
use tauri::Manager;
use tracing_subscriber::EnvFilter;

/// SQLite 数据库文件名
const DB_FILENAME: &str = "smilex-remote-terminal.db";

/// 解析数据库路径：`{app_data_dir}/smilex-remote-terminal.db`
///
/// 优先使用 Tauri 提供的 OS 应用数据目录（跨平台规范）。
fn resolve_db_path(app: &tauri::AppHandle) -> anyhow::Result<PathBuf> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| anyhow::anyhow!("获取 app_data_dir 失败: {e}"))?;
    std::fs::create_dir_all(&dir)
        .map_err(|e| anyhow::anyhow!("创建 app_data_dir 失败: {e}"))?;
    Ok(dir.join(DB_FILENAME))
}

fn main() {
    // 初始化日志
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::from_default_env().add_directive("info".parse().unwrap()),
        )
        .init();

    tracing::info!("SmileX Remote Terminal 启动中...");

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            // 初始化 SQLite（在 setup 中拿到 app_handle 后才能解析路径）
            let db_path = resolve_db_path(&app.handle())?;
            tracing::info!(?db_path, "正在打开 SQLite 数据库");
            let storage = smilex_desktop::storage::sqlite::SqliteStorage::open(db_path)?;

            // 注册全局状态
            let state = AppState::new(storage);
            app.manage(state);

            // 启动时从持久化加载激活的 LLM 配置并应用到 ChatProvider
            // 失败不中断启动（用户可在设置页重新配置）
            // 阶段 6：切换为多档案模式（自动迁移旧版单 key 配置）
            let state_ref: tauri::State<AppState> = app.state();
            // 注意：Tauri 2 的 setup 闭包不在 Tokio runtime 上下文中，
            // 直接调用 `tokio::runtime::Handle::current()` 会 panic
            // ("there is no reactor running")。必须走 Tauri 自带的 async runtime。
            tauri::async_runtime::block_on(async {
                commands::llm_profile::load_and_apply_active_profile(&state_ref).await;
            });

            tracing::info!("SmileX Remote Terminal 已启动");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // SSH 命令组
            commands::session::session_connect,
            commands::session::session_test,
            commands::session::session_input,
            commands::session::session_resize,
            commands::session::session_disconnect,
            commands::session::session_exec,
            commands::session::session_stats_all,
            commands::session::host_key_respond,
            // 监控命令组
            commands::monitor::monitor_start,
            commands::monitor::monitor_stop,
            commands::monitor::monitor_list,
            // SFTP 命令组
            commands::sftp::sftp_list,
            commands::sftp::sftp_mkdir,
            commands::sftp::sftp_remove,
            commands::sftp::sftp_rename,
            commands::sftp::sftp_chmod,
            // SFTP 传输队列（上传/下载/暂停/恢复/取消）
            commands::transfer::sftp_transfer_upload,
            commands::transfer::sftp_transfer_download,
            commands::transfer::sftp_transfer_pause,
            commands::transfer::sftp_transfer_resume,
            commands::transfer::sftp_transfer_cancel,
            commands::transfer::sftp_transfer_retry,
            commands::transfer::sftp_transfers,
            commands::transfer::sftp_transfers_clear,
            // 命令片段 & 历史
            commands::snippet::snippet_list,
            commands::snippet::snippet_save,
            commands::snippet::snippet_delete,
            commands::snippet::snippet_reorder,
            commands::snippet::history_add,
            commands::snippet::history_list,
            commands::snippet::history_clear,
            // 告警规则
            commands::alert::alert_rule_list,
            commands::alert::alert_rule_save,
            commands::alert::alert_rule_delete,
            // SSH 密钥管理
            commands::ssh_key::ssh_key_list,
            commands::ssh_key::ssh_key_generate,
            commands::ssh_key::ssh_key_import,
            commands::ssh_key::ssh_key_delete,
            commands::ssh_key::ssh_key_get_private,
            // 端口转发隧道
            commands::tunnel::tunnel_start,
            commands::tunnel::tunnel_stop,
            commands::tunnel::tunnel_list,
            // 会话配置命令组
            commands::session_profile::session_profile_save,
            commands::session_profile::session_profile_list,
            commands::session_profile::session_profile_get,
            commands::session_profile::session_profile_get_secret,
            commands::session_profile::session_profile_delete,
            commands::session_profile::session_profile_touch,
            // 本机 known_hosts 扫描导入
            commands::known_hosts_import::known_hosts_scan,
            // 远程桌面命令组
            commands::desktop::desktop_connect,
            commands::desktop::desktop_input,
            commands::desktop::desktop_disconnect,
            // AI 命令组
            commands::ai::ai_chat_send,
            commands::ai::ai_chat_abort,
            commands::ai::ai_chat_clear,
            commands::ai::ai_update_config,
            commands::ai::ai_config_get,
            commands::ai::ai_config_save,
            commands::ai::ai_secret_get,
            // Agent 助手会话（多会话 Tab）
            commands::agent_chat::agent_chat_list,
            commands::agent_chat::agent_chat_create,
            commands::agent_chat::agent_chat_rename,
            commands::agent_chat::agent_chat_delete,
            commands::agent_chat::agent_chat_messages,
            commands::agent_chat::agent_chat_message_append,
            commands::agent_chat::agent_chat_clear_messages,
            // LLM 配置档案命令组（阶段 6：多档案管理）
            commands::llm_profile::llm_profile_list,
            commands::llm_profile::llm_profile_get,
            commands::llm_profile::llm_profile_save,
            commands::llm_profile::llm_profile_delete,
            commands::llm_profile::llm_profile_get_api_key,
            commands::llm_profile::llm_profile_set_active,
            commands::llm_profile::llm_profile_get_active,
            commands::llm_profile::llm_profile_test,
            // 通用
            commands::common::app_version,
        ])
        .on_window_event(|window, event| {
            // 应用退出时清理所有会话
            if let tauri::WindowEvent::CloseRequested { .. } = event {
                let state: tauri::State<AppState> = window.app_handle().state();
                // 同 setup：用 Tauri async runtime，避免 Handle::current() panic
                tauri::async_runtime::block_on(async {
                    state.cleanup().await;
                });
            }
        })
        .run(tauri::generate_context!())
        .expect("运行 Tauri 应用失败");
}
