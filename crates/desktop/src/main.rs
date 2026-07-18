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
        .setup(|app| {
            // 初始化 SQLite（在 setup 中拿到 app_handle 后才能解析路径）
            let db_path = resolve_db_path(&app.handle())?;
            tracing::info!(?db_path, "正在打开 SQLite 数据库");
            let storage = smilex_desktop::storage::sqlite::SqliteStorage::open(db_path)?;

            // 注册全局状态
            let state = AppState::new(storage);
            app.manage(state);

            // 启动时从持久化加载 LLM 配置并应用到 ChatProvider
            // 失败不中断启动（用户可在设置页重新配置）
            let state_ref: tauri::State<AppState> = app.state();
            let rt = tokio::runtime::Handle::current();
            rt.block_on(async {
                commands::ai::load_and_apply_llm_config(&state_ref).await;
            });

            tracing::info!("SmileX Remote Terminal 已启动");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // SSH 命令组
            commands::session::session_connect,
            commands::session::session_input,
            commands::session::session_resize,
            commands::session::session_disconnect,
            // 会话配置命令组
            commands::session_profile::session_profile_save,
            commands::session_profile::session_profile_list,
            commands::session_profile::session_profile_get,
            commands::session_profile::session_profile_get_secret,
            commands::session_profile::session_profile_delete,
            commands::session_profile::session_profile_touch,
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
            // 通用
            commands::common::app_version,
        ])
        .on_window_event(|window, event| {
            // 应用退出时清理所有会话
            if let tauri::WindowEvent::CloseRequested { .. } = event {
                let state: tauri::State<AppState> = window.app_handle().state();
                let rt = tokio::runtime::Handle::current();
                rt.block_on(async {
                    state.cleanup().await;
                });
            }
        })
        .run(tauri::generate_context!())
        .expect("运行 Tauri 应用失败");
}
