//! SFTP 传输链路端到端集成测试
//!
//! 直接驱动 [`smilex_desktop::transfer::TransferManager`]（tauri MockRuntime
//! 提供 AppHandle），连本地密钥认证 SSH 服务器真实跑 SFTP 流式传输，
//! 覆盖：上传/下载完整性、暂停/恢复断点续传、取消/重试、会话断开联动、
//! 文件夹递归双向、空目录上传、下载重名去重、流式内存上界。
//!
//! 需要外部测试服务器（不随 cargo test 自动拉起），未提供时全部跳过：
//!
//! ```sh
//! # 1) 起一个仅密钥认证的 sshd（StrictModes off 因 /tmp 粘滞位）
//! #    配置见仓库脚本；关键字段：Port 2222、AuthorizedKeysFile 指向测试目录
//! # 2) 准备 /tmp/srt-testdata（大文件 + 嵌套目录 + 空目录）
//! # 3) 运行（--test-threads=1 保证 RSS 采样与远端目录互不干扰）
//! SRT_E2E=1 cargo test -p smilex-desktop --test transfer_e2e -- --test-threads=1 --nocapture
//! ```
//!
//! 环境变量：`SRT_E2E_PORT`（默认 2222）、`SRT_E2E_KEY`（默认
//! /tmp/srt-sshd/id_ed25519）、`SRT_E2E_USER`（默认 $USER）。

use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use ssh_core::connection::{AuthMethod, ConnectionConfig, SshSession};
use ssh_core::known_hosts::InMemoryKnownHosts;
use smilex_desktop::events::{TransferEventPayload, TransferStatus};
use smilex_desktop::transfer::{TransferManager, TRANSFER_EVENT};
use tauri::Listener;

const SESSION_ID: &str = "e2e-session";
const MB: u64 = 1024 * 1024;

fn e2e_enabled() -> bool {
    std::env::var("SRT_E2E").map(|v| v == "1").unwrap_or(false)
}

fn server_port() -> u16 {
    std::env::var("SRT_E2E_PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(2222)
}

fn server_user() -> String {
    std::env::var("SRT_E2E_USER")
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_else(|_| "smilex".into())
}

fn server_key() -> String {
    std::env::var("SRT_E2E_KEY").unwrap_or_else(|_| "/tmp/srt-sshd/id_ed25519".into())
}

/// 建立 SSH 连接并打开 SFTP 通道（校验通道可用，及早失败）
async fn connect() -> Arc<SshSession> {
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
    let session = Arc::new(
        SshSession::connect(&config, Arc::new(InMemoryKnownHosts::new()), None)
            .await
            .expect("连接测试服务器失败（确认 sshd 已在对应端口运行）"),
    );
    session
        .open_sftp()
        .await
        .expect("打开 SFTP 通道失败");
    session
}

/// MockRuntime AppHandle + 事件收集器（App 必须存活，随返回值持有）
fn mock_with_events() -> (
    tauri::App<tauri::test::MockRuntime>,
    Arc<Mutex<Vec<TransferEventPayload>>>,
) {
    let app = tauri::test::mock_app();
    let events: Arc<Mutex<Vec<TransferEventPayload>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = events.clone();
    app.handle()
        .listen_any(TRANSFER_EVENT, move |e| {
            if let Ok(p) = serde_json::from_str(e.payload()) {
                sink.lock().unwrap().push(p);
            }
        });
    (app, events)
}

/// 轮询快照直到谓词满足（返回满足时的快照）
async fn wait_group(
    mgr: &TransferManager,
    group_id: &str,
    pred: impl Fn(&TransferEventPayload) -> bool,
    timeout: Duration,
) -> TransferEventPayload {
    let start = Instant::now();
    loop {
        let snap = mgr
            .snapshot(Some(SESSION_ID))
            .await
            .into_iter()
            .find(|p| p.group_id == group_id);
        if let Some(p) = &snap {
            if pred(p) {
                return p.clone();
            }
        }
        if start.elapsed() > timeout {
            panic!("等待组 {group_id} 超时（{:?}），当前快照: {snap:?}", timeout);
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

fn finished(p: &TransferEventPayload) -> bool {
    matches!(
        p.status,
        TransferStatus::Completed | TransferStatus::Failed | TransferStatus::Canceled
    )
}

fn assert_completed(p: &TransferEventPayload) {
    assert_eq!(
        format!("{:?}", p.status),
        "Completed",
        "预期完成，实际 {:?}，error: {:?}",
        p.status,
        p.error
    );
}

fn sha256(path: &Path) -> String {
    let out = Command::new("shasum")
        .args(["-a", "256"])
        .arg(path)
        .output()
        .expect("执行 shasum 失败");
    assert!(out.status.success(), "shasum 失败: {}", out.status);
    String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .next()
        .unwrap()
        .to_string()
}

/// 当前测试进程 RSS（KB）
fn rss_kb() -> u64 {
    let out = Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .expect("执行 ps 失败");
    String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse()
        .unwrap_or(0)
}

/// 诊断：单通道累计 ~1GiB 停滞后，区分会话级 / 通道级 / 连接级问题
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn diag_stall_behavior() {
    if std::env::var("SRT_DIAG").map(|v| v == "1").unwrap_or(false) == false {
        eprintln!("SRT_DIAG 未设置，跳过");
        return;
    }
    let session = connect().await;
    let client = session.open_sftp().await.unwrap();
    let _ = client.remove_recursive("/tmp/srt-e2e/diag").await;
    client.mkdir_recursive("/tmp/srt-e2e/diag").await.unwrap();

    // 1.2GB 触发停滞
    let big = "/tmp/srt-testdata/big-1200m.bin";
    if !Path::new(big).exists() {
        let ok = Command::new("mkfile")
            .args(["-n", "1200m", big])
            .status()
            .unwrap()
            .success();
        assert!(ok, "mkfile 失败");
    }

    let cancel = tokio_util::sync::CancellationToken::new();
    let progress = Arc::new(AtomicU64::new(0));
    let up_client = client.clone();
    let up_cancel = cancel.clone();
    let up_progress = progress.clone();
    let upload = tokio::spawn(async move {
        let r = up_client
            .upload_file(Path::new(big), "/tmp/srt-e2e/diag/big.bin", 0, &up_cancel, &up_progress)
            .await;
        eprintln!("upload 结果: {r:?}");
    });

    // 监控：20 秒无进展视为停滞
    let mut last = 0u64;
    let mut stall_at = Instant::now();
    let mut stalled = false;
    let deadline = Instant::now() + Duration::from_secs(900);
    loop {
        tokio::time::sleep(Duration::from_millis(500)).await;
        let cur = progress.load(Ordering::Relaxed);
        if cur != last {
            last = cur;
            stall_at = Instant::now();
        } else if last > 100 * MB && stall_at.elapsed() > Duration::from_secs(20) {
            stalled = true;
            break;
        }
        if Instant::now() > deadline {
            break;
        }
    }
    if !stalled {
        eprintln!("未观察到停滞（progress={last}），结束诊断");
        upload.abort();
        return;
    }
    eprintln!("=== 停滞在 {last} 字节，开始探活 ===");

    // 探活 1：同一 SftpSession 通道上的普通操作
    match tokio::time::timeout(Duration::from_secs(10), client.list("/tmp/srt-e2e/diag")).await {
        Ok(r) => eprintln!("探活1 同通道 list: {:?}", r.map(|v| v.len())),
        Err(_) => eprintln!("探活1 同通道 list: 超时挂住"),
    }
    // 探活 2：同一 SSH 连接开新 SFTP 通道
    let sess2 = session.clone();
    match tokio::time::timeout(Duration::from_secs(10), sess2.open_sftp_channel()).await {
        Ok(r) => eprintln!("探活2 新 SFTP 通道: {:?}", r.map(|_| ()).map_err(|e| e.to_string())),
        Err(_) => eprintln!("探活2 新 SFTP 通道: 超时挂住"),
    }
    // 探活 3：全新 SSH 连接
    match tokio::time::timeout(Duration::from_secs(10), connect()).await {
        Ok(_) => eprintln!("探活3 新 SSH 连接: 成功"),
        Err(_) => eprintln!("探活3 新 SSH 连接: 超时挂住"),
    }

    cancel.cancel();
    // 停滞中的 write_all 观察不到取消令牌，直接丢弃任务
    upload.abort();
    panic!("诊断完成（见上方探活输出）");
}

/// 对照组：1GB 纯上传（无暂停/恢复），定位尾部停滞是否与断点续传相关
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn plain_1g_upload_completes() {
    if !e2e_enabled() {
        eprintln!("SRT_E2E 未设置，跳过");
        return;
    }
    let session = connect().await;
    let client = session.open_sftp().await.unwrap();
    let remote_dir = "/tmp/srt-e2e/plain";
    let _ = client.remove_recursive(remote_dir).await;
    client.mkdir_recursive(remote_dir).await.unwrap();

    let (app, _events) = mock_with_events();
    let mgr = TransferManager::new();
    let src = "/tmp/srt-testdata/big-1g.bin";
    let src_size = std::fs::metadata(src).unwrap().len();

    let ids = mgr
        .add_upload(
            app.handle(),
            SESSION_ID,
            None,
            client.clone(),
            vec![src.into()],
            remote_dir.into(),
        )
        .await
        .unwrap();
    let p = wait_group(&mgr, &ids[0], finished, Duration::from_secs(600)).await;
    assert_completed(&p);
    assert_eq!(p.bytes_done, src_size);
    let st = client.stat(&format!("{remote_dir}/big-1g.bin")).await.unwrap();
    assert_eq!(st.size, src_size);

    session.disconnect().await.unwrap();
}

/// 上传 300MB 随机文件 → 远端大小一致 → 下载回来 sha256 一致（双向完整性），
/// 并校验完成组快照的聚合字段。
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn upload_download_roundtrip_integrity() {
    if !e2e_enabled() {
        eprintln!("SRT_E2E 未设置，跳过");
        return;
    }
    let session = connect().await;
    let client = session.open_sftp().await.unwrap();
    let remote_dir = "/tmp/srt-e2e/rt";
    let _ = client.remove_recursive(remote_dir).await;
    client.mkdir_recursive(remote_dir).await.unwrap();

    let (app, events) = mock_with_events();
    let mgr = TransferManager::new();
    let src = "/tmp/srt-testdata/rand-300m.bin";
    let src_size = std::fs::metadata(src).unwrap().len();

    let ids = mgr
        .add_upload(
            app.handle(),
            SESSION_ID,
            Some("e2e-label".into()),
            client.clone(),
            vec![src.into()],
            remote_dir.into(),
        )
        .await
        .unwrap();
    let done = wait_group(&mgr, &ids[0], finished, Duration::from_secs(240)).await;
    assert_completed(&done);

    let remote_file = format!("{remote_dir}/rand-300m.bin");
    let st = client.stat(&remote_file).await.unwrap();
    assert_eq!(st.size, src_size, "远端文件大小应与源一致");

    // 下载回来对比哈希
    let save = "/tmp/srt-e2e-down/rt";
    let _ = std::fs::remove_dir_all(save);
    let ids = mgr
        .add_download(
            app.handle(),
            SESSION_ID,
            None,
            client.clone(),
            vec![remote_file],
            Some(save.into()),
        )
        .await
        .unwrap();
    let done = wait_group(&mgr, &ids[0], finished, Duration::from_secs(240)).await;
    assert_completed(&done);

    assert_eq!(
        sha256(&Path::new(save).join("rand-300m.bin")),
        sha256(Path::new(src)),
        "下载回来的文件内容应与源一致"
    );

    // 完成组快照聚合字段
    let upload_snap = mgr
        .snapshot(Some(SESSION_ID))
        .await
        .into_iter()
        .find(|p| p.name == "rand-300m.bin" && p.kind == smilex_desktop::events::TransferKind::Upload)
        .unwrap();
    assert!(!upload_snap.is_dir);
    assert_eq!(upload_snap.file_count, 1);
    assert_eq!(upload_snap.size_total, src_size);
    assert_eq!(upload_snap.bytes_done, src_size);
    assert_eq!(upload_snap.files_done, 1);
    assert_eq!(upload_snap.session_label.as_deref(), Some("e2e-label"));
    assert_eq!(upload_snap.speed_bps, 0, "完成后速度应归零");
    assert!(upload_snap.local_path.is_none(), "上传组无本地保存路径");

    // 事件流：至少有 queued/running 与 completed 快照，且全部为 camelCase 字段可反序列化
    let evts = events.lock().unwrap();
    assert!(
        evts.iter().any(|p| p.status == TransferStatus::Completed),
        "事件流应包含完成快照"
    );
    assert!(
        evts.iter().any(|p| p.status == TransferStatus::Running),
        "事件流应包含传输中快照"
    );

    session.disconnect().await.unwrap();
}

/// 1GB 文件上传中途暂停 → 远端大小 == 暂停时进度；恢复 → 从断点续传完成；
/// 全程 RSS 增量受限（流式不整体进内存）。
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pause_resume_with_offset_and_streaming_memory() {
    if !e2e_enabled() {
        eprintln!("SRT_E2E 未设置，跳过");
        return;
    }
    let session = connect().await;
    let client = session.open_sftp().await.unwrap();
    let remote_dir = "/tmp/srt-e2e/pr";
    let _ = client.remove_recursive(remote_dir).await;
    client.mkdir_recursive(remote_dir).await.unwrap();

    let (app, _events) = mock_with_events();
    let mgr = TransferManager::new();
    let src = "/tmp/srt-testdata/big-1g.bin";
    let src_size = std::fs::metadata(src).unwrap().len();
    let remote_file = format!("{remote_dir}/big-1g.bin");

    let base_rss = rss_kb();
    let peak_rss = Arc::new(AtomicU64::new(base_rss));
    let sampler_peak = peak_rss.clone();
    let sampler = tokio::spawn(async move {
        loop {
            let cur = rss_kb();
            sampler_peak.fetch_max(cur, Ordering::Relaxed);
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    });

    let ids = mgr
        .add_upload(
            app.handle(),
            SESSION_ID,
            None,
            client.clone(),
            vec![src.into()],
            remote_dir.into(),
        )
        .await
        .unwrap();
    let id = ids[0].clone();

    // 等已传过 128MB（仍在传输中）
    wait_group(
        &mgr,
        &id,
        |p| p.status == TransferStatus::Running && p.bytes_done > 128 * MB,
        Duration::from_secs(120),
    )
    .await;

    mgr.pause(app.handle(), &id).await.unwrap();
    let _ = wait_group(
        &mgr,
        &id,
        |p| p.status == TransferStatus::Paused,
        Duration::from_secs(30),
    )
    .await;
    // pause() 的兜底 emit 可能先于 worker 收尾（最后一个块尚未落账），
    // 静置后取稳定快照再校验断点
    tokio::time::sleep(Duration::from_millis(1200)).await;
    let p = mgr
        .snapshot(Some(SESSION_ID))
        .await
        .into_iter()
        .find(|s| s.group_id == id)
        .expect("组应存在");
    assert_eq!(p.status, TransferStatus::Paused);
    assert!(p.bytes_done < src_size, "暂停时应未传完");
    assert!(p.bytes_done >= 128 * MB);

    // 远端已落盘字节 == 暂停时进度（断点续传的对齐基准）
    let st = client.stat(&remote_file).await.unwrap();
    assert_eq!(st.size, p.bytes_done, "远端大小应等于暂停时的进度");

    // 稍作停留后恢复（新 client，模拟用户点击恢复）
    tokio::time::sleep(Duration::from_millis(500)).await;
    mgr.resume(app.handle(), &id, client.clone()).await.unwrap();
    let p = wait_group(&mgr, &id, finished, Duration::from_secs(300)).await;
    assert_completed(&p);
    assert_eq!(p.bytes_done, src_size);

    let st = client.stat(&remote_file).await.unwrap();
    assert_eq!(st.size, src_size, "恢复续传后远端大小应等于源");

    sampler.abort();
    let peak = peak_rss.load(Ordering::Relaxed);
    let growth_kb = peak.saturating_sub(base_rss);
    assert!(
        growth_kb < 200 * 1024,
        "1GB 流式上传期间 RSS 增量应远小于文件大小（实际增长 {growth_kb} KB）"
    );
    eprintln!("基线 RSS {base_rss} KB，峰值 {peak} KB，增量 {growth_kb} KB");

    session.disconnect().await.unwrap();
}

/// 上传中途取消 → 状态 Canceled；重试 → 从断点续传至完成；
/// 会话级取消（断开联动）→ 该会话进行中的组全部 Canceled 且带错误说明。
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cancel_retry_and_session_cancel() {
    if !e2e_enabled() {
        eprintln!("SRT_E2E 未设置，跳过");
        return;
    }
    let session = connect().await;
    let client = session.open_sftp().await.unwrap();
    let remote_dir = "/tmp/srt-e2e/cr";
    let _ = client.remove_recursive(remote_dir).await;
    client.mkdir_recursive(remote_dir).await.unwrap();

    let (app, _events) = mock_with_events();
    let mgr = TransferManager::new();
    let src = "/tmp/srt-testdata/big-1g.bin";
    let src_size = std::fs::metadata(src).unwrap().len();

    // —— 单组取消 → 重试 ——
    let ids = mgr
        .add_upload(
            app.handle(),
            SESSION_ID,
            None,
            client.clone(),
            vec![src.into()],
            remote_dir.into(),
        )
        .await
        .unwrap();
    let id = ids[0].clone();
    wait_group(
        &mgr,
        &id,
        |p| p.status == TransferStatus::Running && p.bytes_done > 64 * MB,
        Duration::from_secs(120),
    )
    .await;
    mgr.cancel(app.handle(), &id).await.unwrap();
    let p = wait_group(
        &mgr,
        &id,
        |p| p.status == TransferStatus::Canceled,
        Duration::from_secs(30),
    )
    .await;
    assert!(p.bytes_done < src_size);

    let remote_file = format!("{remote_dir}/big-1g.bin");
    let st = client.stat(&remote_file).await.unwrap();
    assert!(st.size < src_size, "取消后远端不应完整");

    mgr.retry(app.handle(), &id, client.clone()).await.unwrap();
    // retry 不立即改状态：先等新 worker 把组推入 Running，避免轮询到旧 Canceled 快照
    wait_group(
        &mgr,
        &id,
        |p| p.status == TransferStatus::Running,
        Duration::from_secs(30),
    )
    .await;
    let p = wait_group(&mgr, &id, finished, Duration::from_secs(300)).await;
    assert_completed(&p);
    assert_eq!(p.bytes_done, src_size);
    let st = client.stat(&remote_file).await.unwrap();
    assert_eq!(st.size, src_size, "重试续传后远端应完整");

    // —— 会话级取消（模拟会话断开钩子）——
    let ids2 = mgr
        .add_upload(
            app.handle(),
            SESSION_ID,
            None,
            client.clone(),
            vec!["/tmp/srt-testdata/rand-300m.bin".into()],
            remote_dir.into(),
        )
        .await
        .unwrap();
    wait_group(
        &mgr,
        &ids2[0],
        |p| p.status == TransferStatus::Running && p.bytes_done > 32 * MB,
        Duration::from_secs(120),
    )
    .await;
    mgr.cancel_session(Some(app.handle()), SESSION_ID).await;
    let p = wait_group(
        &mgr,
        &ids2[0],
        |p| p.status == TransferStatus::Canceled,
        Duration::from_secs(30),
    )
    .await;
    assert_eq!(
        p.error.as_deref(),
        Some("会话已断开"),
        "会话取消应带错误说明，实际: {:?}",
        p.error
    );

    session.disconnect().await.unwrap();
}

/// 文件夹递归上传（含嵌套与空目录）→ 远端结构完整；递归下载回来内容一致；
/// 同名重复下载自动去重 `name (1)`。
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn folder_recursive_upload_download_and_dedupe() {
    if !e2e_enabled() {
        eprintln!("SRT_E2E 未设置，跳过");
        return;
    }
    let session = connect().await;
    let client = session.open_sftp().await.unwrap();
    let remote_dir = "/tmp/srt-e2e/fd";
    let _ = client.remove_recursive(remote_dir).await;
    client.mkdir_recursive(remote_dir).await.unwrap();

    let (app, _events) = mock_with_events();
    let mgr = TransferManager::new();
    let src_dir = "/tmp/srt-testdata/nested";

    let ids = mgr
        .add_upload(
            app.handle(),
            SESSION_ID,
            None,
            client.clone(),
            vec![src_dir.into()],
            remote_dir.into(),
        )
        .await
        .unwrap();
    let p = wait_group(&mgr, &ids[0], finished, Duration::from_secs(180)).await;
    assert_completed(&p);
    assert!(p.is_dir, "文件夹组的 is_dir 应为 true");
    assert_eq!(p.file_count, 4, "nested 下应有 4 个文件");
    assert_eq!(p.files_done, 4);
    assert_eq!(
        p.size_total,
        8 * 1024 * 1024 + std::fs::metadata("/tmp/srt-testdata/nested/top.txt").unwrap().len()
            + std::fs::metadata("/tmp/srt-testdata/nested/a/readme.txt").unwrap().len()
            + std::fs::metadata("/tmp/srt-testdata/nested/c/leaf.txt").unwrap().len()
    );

    // 远端结构：4 个文件落位 + 空目录 empty-dir 被创建
    let remote_nested = format!("{remote_dir}/nested");
    let files = client.walk_files(&remote_nested).await.unwrap();
    let mut names: Vec<&str> = files.iter().map(|f| f.name.as_str()).collect();
    names.sort();
    assert_eq!(names, vec!["blob.bin", "leaf.txt", "readme.txt", "top.txt"]);
    let blob_remote = files.iter().find(|f| f.name == "blob.bin").unwrap();
    assert!(blob_remote.path.ends_with("a/b/blob.bin"), "嵌套层级应保留");

    let lst = client.list(&remote_nested).await.unwrap();
    assert!(
        lst.iter().any(|e| e.is_dir && e.name == "empty-dir"),
        "空目录应被创建"
    );

    // 递归下载回来
    let save = "/tmp/srt-e2e-down/fd";
    let _ = std::fs::remove_dir_all(save);
    let ids = mgr
        .add_download(
            app.handle(),
            SESSION_ID,
            None,
            client.clone(),
            vec![remote_nested.clone()],
            Some(save.into()),
        )
        .await
        .unwrap();
    let p = wait_group(&mgr, &ids[0], finished, Duration::from_secs(180)).await;
    assert_completed(&p);
    assert_eq!(sha256(&Path::new(save).join("nested/a/b/blob.bin")),
        sha256(Path::new("/tmp/srt-testdata/nested/a/b/blob.bin")));
    assert!(Path::new(save).join("nested/c/leaf.txt").is_file());

    // 同名再下载一次 → 顶层目录去重为 nested (1)
    let ids = mgr
        .add_download(
            app.handle(),
            SESSION_ID,
            None,
            client.clone(),
            vec![remote_nested],
            Some(save.into()),
        )
        .await
        .unwrap();
    wait_group(&mgr, &ids[0], finished, Duration::from_secs(180)).await;
    assert!(
        Path::new(save).join("nested (1)/top.txt").is_file(),
        "重复下载应去重为 nested (1)"
    );

    session.disconnect().await.unwrap();
}
