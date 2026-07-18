//! # SmileX Remote Terminal 桌面应用入口
//!
//! Tauri 主进程入口，负责：
//! 1. 初始化日志/存储
//! 2. 注册 Tauri commands
//! 3. 拉起应用窗口

#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

use smilex_desktop::{commands, AppState};
// 引入 Manager trait 以便调用 window.app_handle()
use tauri::Manager;
use tracing_subscriber::EnvFilter;

fn main() {
    // 初始化日志
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse().unwrap()))
        .init();

    tracing::info!("SmileX Remote Terminal 启动中...");

    // 构建 Tauri 应用
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            // SSH 命令组
            commands::session::session_connect,
            commands::session::session_input,
            commands::session::session_resize,
            commands::session::session_disconnect,
            // 远程桌面命令组
            commands::desktop::desktop_connect,
            commands::desktop::desktop_input,
            commands::desktop::desktop_disconnect,
            // AI 命令组
            commands::ai::ai_chat_send,
            commands::ai::ai_chat_abort,
            commands::ai::ai_chat_clear,
            commands::ai::ai_update_config,
            // 通用
            commands::common::app_version,
        ])
        .setup(|_app| {
            tracing::info!("SmileX Remote Terminal 已启动");
            Ok(())
        })
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
