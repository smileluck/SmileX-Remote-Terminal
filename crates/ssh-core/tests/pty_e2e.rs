//! PTY 初始输出端到端测试（真实本地 sshd，SRT_E2E=1 门控）
//!
//! 验证分屏克隆场景的后端半边：新建 SSH 会话 + open_pty 后，
//! 远端 shell 的初始提示符应通过 TerminalStream 持续流出。
//!
//! 环境变量与 transfer_e2e 一致：`SRT_E2E_PORT`（默认 2222）、
//! `SRT_E2E_KEY`（默认 /tmp/srt-sshd/id_ed25519）、`SRT_E2E_USER`（默认 $USER）。

use std::sync::Arc;
use std::time::Duration;

use ssh_core::connection::{AuthMethod, ConnectionConfig, SshSession};
use ssh_core::known_hosts::InMemoryKnownHosts;
use ssh_core::terminal::TerminalStream;

fn e2e_enabled() -> bool {
    std::env::var("SRT_E2E").map(|v| v == "1").unwrap_or(false)
}

#[tokio::test]
async fn pty_emits_initial_output() {
    if !e2e_enabled() {
        eprintln!("SRT_E2E 未设置，跳过");
        return;
    }
    let port: u16 = std::env::var("SRT_E2E_PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(2222);
    let key_path =
        std::env::var("SRT_E2E_KEY").unwrap_or_else(|_| "/tmp/srt-sshd/id_ed25519".into());
    let user = std::env::var("SRT_E2E_USER")
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_else(|_| "smilex".into());

    let config = ConnectionConfig {
        host: "127.0.0.1".into(),
        port,
        username: user,
        auth: AuthMethod::PrivateKey {
            path: key_path,
            passphrase: None,
        },
        accept_first_host_key: true,
    };

    // 与前端克隆路径完全一致：connect → open_pty(80,24) → 读循环
    let session = SshSession::connect(&config, Arc::new(InMemoryKnownHosts::new()), None)
        .await
        .expect("连接本地 sshd 失败");
    let pty = session.open_pty(80, 24).await.expect("打开 PTY 失败");
    let (mut stream, _ctrl) = TerminalStream::start(pty);

    // 收集 3 秒输出
    let mut buf: Vec<u8> = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    while let Ok(Some(chunk)) = tokio::time::timeout_at(deadline, stream.rx.recv()).await {
        buf.extend_from_slice(&chunk.data);
    }

    let text = String::from_utf8_lossy(&buf);
    println!("=== 初始输出（{} 字节）===", buf.len());
    println!("{text}");
    assert!(
        !buf.is_empty(),
        "PTY 建立后 3 秒内没有任何输出（提示符未流出）"
    );
}
