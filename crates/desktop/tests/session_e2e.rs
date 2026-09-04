//! session_connect 端到端测试（真实本地 sshd，SRT_E2E=1 门控）
//!
//! 走 MockRuntime 的真实 invoke 管线（与前端 `invoke()` 等价）：
//! session_connect 命令 → PTY → TerminalStream → ipc::Channel 推送。
//! Channel 消息经 `Builder::channel_interceptor` 捕获（官方测试钩子，
//! 见 tauri::test 文档）。验证初始提示符经 Channel 到达、session_input
//! 后有后续输出（排除「新分屏空白」的桌面层后端可能）。

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use ssh_core::connection::{AuthMethod, ConnectionConfig};
use smilex_desktop::events::TerminalOutputPayload;
use smilex_desktop::storage::sqlite::SqliteStorage;
use smilex_desktop::AppState;
use tauri::ipc::{CallbackFn, InvokeBody, InvokeResponseBody};
use tauri::test::{get_ipc_response, mock_builder, mock_context, noop_assets, MockRuntime, INVOKE_KEY};
use tauri::{Webview, WebviewWindowBuilder};
use tauri::webview::InvokeRequest;

fn e2e_enabled() -> bool {
    std::env::var("SRT_E2E").map(|v| v == "1").unwrap_or(false)
}

fn server_port() -> u16 {
    std::env::var("SRT_E2E_PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(2222)
}

fn server_key() -> String {
    std::env::var("SRT_E2E_KEY").unwrap_or_else(|_| "/tmp/srt-sshd/id_ed25519".into())
}

fn server_user() -> String {
    std::env::var("SRT_E2E_USER")
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_else(|_| "smilex".into())
}

/// 发起一次与前端等价的 invoke；Ok 返回命令结果的 JSON 值
fn invoke(
    webview: &tauri::WebviewWindow<MockRuntime>,
    cmd: &str,
    body: serde_json::Value,
) -> Result<serde_json::Value, serde_json::Value> {
    let req = InvokeRequest {
        cmd: cmd.into(),
        callback: CallbackFn(0),
        error: CallbackFn(1),
        // macOS/Linux 本地 origin 为 tauri://，Windows/Android 为 http://tauri.localhost
        // （非本地 origin 会触发 ACL 拒绝：app 命令仅对本地 origin 免 ACL）
        url: if cfg!(any(windows, target_os = "android")) {
            "http://tauri.localhost"
        } else {
            "tauri://localhost"
        }
        .parse()
        .unwrap(),
        body: InvokeBody::Json(body),
        headers: Default::default(),
        invoke_key: INVOKE_KEY.to_string(),
    };
    get_ipc_response(webview, req).map(|b| b.deserialize().unwrap())
}

/// 轮询直到谓词满足或超时
fn wait_for(pred: impl Fn() -> bool, timeout: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if pred() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    pred()
}

#[test]
fn session_connect_emits_initial_output_via_channel() {
    if !e2e_enabled() {
        eprintln!("SRT_E2E 未设置，跳过");
        return;
    }

    // 捕获 Channel 推送的终端输出（TerminalOutputPayload 序列化后的 JSON）
    let buf: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = buf.clone();

    let app = mock_builder()
        .invoke_handler(tauri::generate_handler![
            smilex_desktop::commands::session::session_connect,
            smilex_desktop::commands::session::session_input,
            smilex_desktop::commands::session::session_disconnect
        ])
        .channel_interceptor(
            move |_wv: &Webview<MockRuntime>,
                  _cb: CallbackFn,
                  _idx: usize,
                  body: &InvokeResponseBody|
                  -> bool {
                if let InvokeResponseBody::Json(s) = body {
                    if let Ok(p) = serde_json::from_str::<TerminalOutputPayload>(s) {
                        sink.lock().unwrap().extend_from_slice(&p.data);
                    }
                }
                true // 已消费，不再走默认 eval 路径
            },
        )
        .manage(AppState::new(
            // open_in_memory 仅 cfg(test) 可见（集成测试不可达），用临时文件库
            SqliteStorage::open(std::env::temp_dir().join(format!(
                "srt-session-e2e-{}.db",
                std::process::id()
            )))
            .unwrap(),
        ))
        .build(mock_context(noop_assets()))
        .expect("构建 mock app 失败");
    let _app_guard = &app; // App 必须存活（webview 持其 manager 引用）

    let webview = WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .expect("创建 mock webview 失败");

    let config = ConnectionConfig {
        host: "127.0.0.1".into(),
        port: server_port(),
        username: server_user(),
        auth: AuthMethod::PrivateKey {
            path: server_key(),
            passphrase: None,
        },
        accept_first_host_key: true,
    };

    // 与前端一致：参数 camelCase，Channel 以 "__CHANNEL__:id" 字符串传入
    let sid = invoke(
        &webview,
        "session_connect",
        serde_json::json!({
            "config": serde_json::to_value(&config).unwrap(),
            "cols": 80,
            "rows": 24,
            "onOutput": "__CHANNEL__:9",
        }),
    )
    .expect("session_connect 失败")
    .as_str()
    .expect("session_connect 应返回 sessionId 字符串")
    .to_string();
    println!("sessionId = {sid}");

    // 等初始提示符（Last login / PS1）经 Channel 到达
    assert!(
        wait_for(|| !buf.lock().unwrap().is_empty(), Duration::from_secs(5)),
        "session_connect 后 Channel 未收到任何初始输出"
    );
    let initial = buf.lock().unwrap().clone();
    println!("=== Channel 初始输出（{} 字节）===", initial.len());
    println!("{}", String::from_utf8_lossy(&initial));

    // 输入一条命令，验证链路双向通
    let input_data: Vec<u8> = "echo srt-e2e-ok\n".bytes().collect();
    invoke(
        &webview,
        "session_input",
        serde_json::json!({ "sessionId": sid, "data": input_data }),
    )
    .expect("session_input 失败");

    assert!(
        wait_for(
            || String::from_utf8_lossy(&buf.lock().unwrap()).contains("srt-e2e-ok"),
            Duration::from_secs(5)
        ),
        "回显命令未出现在 Channel 输出中"
    );
    println!(
        "=== 输入后累计输出 ===\n{}",
        String::from_utf8_lossy(&buf.lock().unwrap())
    );

    invoke(
        &webview,
        "session_disconnect",
        serde_json::json!({ "sessionId": sid }),
    )
    .expect("session_disconnect 失败");
}
