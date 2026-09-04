//! # SFTP 传输管理器
//!
//! 文件/文件夹上传下载的队列、进度、暂停/恢复/取消管理（UU远程式传输管理）。
//!
//! 设计：
//! - 每个顶层条目（一个文件或一个文件夹）为一个 `TransferGroup`，
//!   文件夹在入队时展开为子文件任务（总量预知 → 精确进度）
//! - 组内文件串行传输，组间并发受 `Semaphore`（2 个许可）限制
//! - 数据由 [`ssh_core::sftp::SftpClient`] 分块流式搬运，不进内存整体缓冲
//! - 进度通过 `transfer_event` 全局事件推送（快照式 payload，前端按
//!   groupId upsert 即可），速度由独立的采样任务每 250ms 计算一次

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tauri::{AppHandle, Emitter, Runtime};
use tokio::sync::{watch, Mutex, Semaphore};
use tokio_util::sync::CancellationToken;

use ssh_core::sftp::SftpClient;

use crate::error::AppError;
use crate::events::{TransferEventPayload, TransferKind, TransferStatus};

/// 前端监听的传输事件名
pub const TRANSFER_EVENT: &str = "transfer_event";

/// 组运行控制指令
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ctrl {
    Run,
    Pause,
    Cancel,
}

/// 子树无文件的目录（文件夹递归传输时需显式创建，否则结构丢失）
#[derive(Debug, Clone)]
enum EmptyDir {
    /// 上传：远端待创建目录
    Remote(String),
    /// 下载：本地待创建目录
    Local(PathBuf),
}

/// 组内单个文件任务（跨 worker 运行共享：暂停/重试后从 bytes_done 续传）
struct FileTask {
    local_path: PathBuf,
    remote_path: String,
    size_total: u64,
    bytes_done: AtomicU64,
    done: AtomicBool,
}

impl FileTask {
    fn new(local_path: PathBuf, remote_path: String, size_total: u64) -> Arc<Self> {
        Arc::new(Self {
            local_path,
            remote_path,
            size_total,
            bytes_done: AtomicU64::new(0),
            done: AtomicBool::new(false),
        })
    }
}

/// 传输组：一个顶层文件/文件夹
pub struct TransferGroup {
    pub id: String,
    pub session_id: String,
    pub session_label: Option<String>,
    pub kind: TransferKind,
    pub name: String,
    pub is_dir: bool,
    /// 下载：本地目标路径；上传：None
    pub local_base: Option<PathBuf>,
    /// 上传：远端目标路径（remote_dir/name，空文件夹创建用）；下载：None
    pub remote_root: Option<String>,
    files: Vec<Arc<FileTask>>,
    /// 子树无文件的目录（文件传完后补建，保留空目录结构）
    empty_dirs: Vec<EmptyDir>,
    size_total: u64,
    bytes_done: AtomicU64,
    files_done: AtomicU32,
    file_count: u32,
    status: Mutex<TransferStatus>,
    error: Mutex<Option<String>>,
    speed_bps: AtomicU64,
    created_at_ms: u64,
    control_tx: watch::Sender<Ctrl>,
    /// 当前 worker 运行的取消令牌（每次运行一个新 token）
    current_cancel: Mutex<Option<CancellationToken>>,
}

impl TransferGroup {
    fn aggregate(&self) -> (u64, u32) {
        let mut bytes = 0u64;
        let mut done = 0u32;
        for f in &self.files {
            bytes += f.bytes_done.load(Ordering::Relaxed);
            if f.done.load(Ordering::Relaxed) {
                done += 1;
            }
        }
        (bytes, done)
    }

    fn refresh_aggregates(&self) {
        let (bytes, done) = self.aggregate();
        self.bytes_done.store(bytes, Ordering::Relaxed);
        self.files_done.store(done, Ordering::Relaxed);
    }

    async fn set_status(&self, status: TransferStatus) {
        *self.status.lock().await = status;
    }

    fn snapshot(&self) -> TransferEventPayload {
        let status = self
            .status
            .try_lock()
            .map(|s| *s)
            .unwrap_or(TransferStatus::Running);
        let error = self.error.try_lock().ok().and_then(|e| e.clone());
        TransferEventPayload {
            session_id: self.session_id.clone(),
            session_label: self.session_label.clone(),
            group_id: self.id.clone(),
            kind: self.kind,
            name: self.name.clone(),
            is_dir: self.is_dir,
            status,
            bytes_done: self.bytes_done.load(Ordering::Relaxed),
            size_total: self.size_total,
            speed_bps: self.speed_bps.load(Ordering::Relaxed),
            files_done: self.files_done.load(Ordering::Relaxed),
            file_count: self.file_count,
            error,
            local_path: self
                .local_base
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned()),
            created_at_ms: self.created_at_ms,
        }
    }
}

/// 推送一次组快照事件
fn emit_group<R: Runtime>(app: &AppHandle<R>, group: &TransferGroup) {
    group.refresh_aggregates();
    let _ = app.emit(TRANSFER_EVENT, group.snapshot());
}

/// 远端路径拼接（POSIX）
fn remote_join(dir: &str, name: &str) -> String {
    format!("{}/{}", dir.trim_end_matches('/'), name)
}

/// 远端父目录
fn remote_parent(path: &str) -> String {
    match path.rfind('/') {
        Some(0) | None => "/".to_string(),
        Some(i) => path[..i].to_string(),
    }
}

/// 本地保存文件名去重：name.ext → name (1).ext → name (2).ext …
fn dedupe_path(dir: &Path, name: &str) -> PathBuf {
    let candidate = dir.join(name);
    if !candidate.exists() {
        return candidate;
    }
    let stem = Path::new(name)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| name.to_string());
    let ext = Path::new(name)
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    for i in 1..u32::MAX {
        let candidate = dir.join(format!("{stem} ({i}){ext}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    candidate
}

/// 递归列举本地目录文件（跳过符号链接），返回 (绝对路径, 大小) 与子树无文件的空目录
fn walk_local(root: &Path) -> std::io::Result<(Vec<(PathBuf, u64)>, Vec<PathBuf>)> {
    let mut files = Vec::new();
    let mut dirs = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let lmeta = std::fs::symlink_metadata(entry.path())?;
            if lmeta.file_type().is_symlink() {
                continue;
            }
            if lmeta.is_dir() {
                dirs.push(entry.path());
                stack.push(entry.path());
            } else {
                files.push((entry.path(), lmeta.len()));
            }
        }
    }
    let empty_dirs = dirs
        .into_iter()
        .filter(|d| !files.iter().any(|(f, _)| f.starts_with(d)))
        .collect();
    Ok((files, empty_dirs))
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// SFTP 传输管理器
pub struct TransferManager {
    groups: Mutex<HashMap<String, Arc<TransferGroup>>>,
    /// 并发传输许可（组间并发上限）
    semaphore: Arc<Semaphore>,
}

impl Default for TransferManager {
    fn default() -> Self {
        Self::new()
    }
}

impl TransferManager {
    pub fn new() -> Self {
        Self {
            groups: Mutex::new(HashMap::new()),
            semaphore: Arc::new(Semaphore::new(2)),
        }
    }

    /// 新建上传组（本地路径数组 → remote_dir），返回组 ID 列表
    pub async fn add_upload<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        session_id: &str,
        session_label: Option<String>,
        client: SftpClient,
        local_paths: Vec<String>,
        remote_dir: String,
    ) -> Result<Vec<String>, AppError> {
        let mut ids = Vec::new();
        for lp in local_paths {
            let path = PathBuf::from(&lp);
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or(lp.clone());
            let lmeta = std::fs::symlink_metadata(&path)
                .map_err(|e| AppError::session(format!("读取本地路径 {lp} 失败: {e}")))?;
            let is_symlink = lmeta.file_type().is_symlink();

            let mut files: Vec<Arc<FileTask>> = Vec::new();
            let mut empty_dirs: Vec<EmptyDir> = Vec::new();
            let is_dir;
            if lmeta.is_dir() && !is_symlink {
                is_dir = true;
                let (walked, empties) = walk_local(&path)
                    .map_err(|e| AppError::session(format!("扫描本地目录 {lp} 失败: {e}")))?;
                for (fp, size) in walked {
                    let rel = fp
                        .strip_prefix(&path)
                        .map(|r| r.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    let remote = remote_join(
                        &remote_join(&remote_dir, &name),
                        &rel.replace('\\', "/"),
                    );
                    files.push(FileTask::new(fp, remote, size));
                }
                for ep in empties {
                    let rel = ep
                        .strip_prefix(&path)
                        .map(|r| r.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    empty_dirs.push(EmptyDir::Remote(remote_join(
                        &remote_join(&remote_dir, &name),
                        &rel.replace('\\', "/"),
                    )));
                }
            } else {
                // 普通文件或符号链接（按文件上传，读取目标内容）
                is_dir = false;
                let meta = std::fs::metadata(&path)
                    .map_err(|e| AppError::session(format!("读取本地文件 {lp} 失败: {e}")))?;
                files.push(FileTask::new(
                    path.clone(),
                    remote_join(&remote_dir, &name),
                    meta.len(),
                ));
            }

            let group = self
                .create_group(
                    session_id,
                    session_label.clone(),
                    TransferKind::Upload,
                    name.clone(),
                    is_dir,
                    None,
                    Some(remote_join(&remote_dir, &name)),
                    files,
                    empty_dirs,
                )
                .await;
            emit_group(app, &group);
            spawn_worker(app.clone(), self.semaphore.clone(), group.clone(), client.clone()).await;
            ids.push(group.id.clone());
        }
        Ok(ids)
    }

    /// 新建下载组（远端路径数组 → save_dir 或默认下载目录），返回组 ID 列表
    pub async fn add_download<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        session_id: &str,
        session_label: Option<String>,
        client: SftpClient,
        remote_paths: Vec<String>,
        save_dir: Option<String>,
    ) -> Result<Vec<String>, AppError> {
        let base_dir = match save_dir {
            Some(d) => PathBuf::from(d),
            None => dirs::download_dir()
                .or_else(dirs::home_dir)
                .ok_or_else(|| AppError::new(crate::error::ErrorCode::Internal, "无法定位本地下载目录"))?,
        };
        let _ = tokio::fs::create_dir_all(&base_dir).await;

        let mut ids = Vec::new();
        for rp in remote_paths {
            let st = client
                .stat(&rp)
                .await
                .map_err(|e| AppError::session(format!("查询远端路径 {rp} 失败: {e}")))?;
            let name = st.name.clone();

            let mut files: Vec<Arc<FileTask>> = Vec::new();
            let mut empty_dirs: Vec<EmptyDir> = Vec::new();
            let local_base: PathBuf;
            let is_dir;
            if st.is_dir {
                is_dir = true;
                // 顶层目录名去重，避免覆盖已有下载
                local_base = dedupe_path(&base_dir, &name);
                let (walked, empties) = client
                    .walk_tree(&rp)
                    .await
                    .map_err(|e| AppError::session(format!("扫描远端目录 {rp} 失败: {e}")))?;
                let root_len = rp.trim_end_matches('/').len();
                for entry in walked {
                    let rel = &entry.path[root_len..]; // 以 / 开头的相对路径
                    let local = local_base.join(rel.trim_start_matches('/'));
                    files.push(FileTask::new(local, entry.path.clone(), entry.size));
                }
                for ep in empties {
                    let rel = &ep[root_len..];
                    empty_dirs.push(EmptyDir::Local(local_base.join(rel.trim_start_matches('/'))));
                }
            } else {
                is_dir = false;
                local_base = dedupe_path(&base_dir, &name);
                files.push(FileTask::new(
                    local_base.clone(),
                    rp.clone(),
                    st.size,
                ));
            }

            let group = self
                .create_group(
                    session_id,
                    session_label.clone(),
                    TransferKind::Download,
                    name,
                    is_dir,
                    Some(local_base),
                    None,
                    files,
                    empty_dirs,
                )
                .await;
            emit_group(app, &group);
            spawn_worker(app.clone(), self.semaphore.clone(), group.clone(), client.clone()).await;
            ids.push(group.id.clone());
        }
        Ok(ids)
    }

    #[allow(clippy::too_many_arguments)]
    async fn create_group(
        &self,
        session_id: &str,
        session_label: Option<String>,
        kind: TransferKind,
        name: String,
        is_dir: bool,
        local_base: Option<PathBuf>,
        remote_root: Option<String>,
        files: Vec<Arc<FileTask>>,
        empty_dirs: Vec<EmptyDir>,
    ) -> Arc<TransferGroup> {
        let size_total = files.iter().map(|f| f.size_total).sum();
        let file_count = files.len() as u32;
        let (control_tx, _) = watch::channel(Ctrl::Run);
        let group = Arc::new(TransferGroup {
            id: uuid::Uuid::new_v4().to_string(),
            session_id: session_id.to_string(),
            session_label,
            kind,
            name,
            is_dir,
            local_base,
            remote_root,
            files,
            empty_dirs,
            size_total,
            bytes_done: AtomicU64::new(0),
            files_done: AtomicU32::new(0),
            file_count,
            status: Mutex::new(TransferStatus::Queued),
            error: Mutex::new(None),
            speed_bps: AtomicU64::new(0),
            created_at_ms: now_ms(),
            control_tx,
            current_cancel: Mutex::new(None),
        });
        self.groups
            .lock()
            .await
            .insert(group.id.clone(), group.clone());
        group
    }

    async fn get_group(&self, group_id: &str) -> Result<Arc<TransferGroup>, AppError> {
        self.groups
            .lock()
            .await
            .get(group_id)
            .cloned()
            .ok_or_else(|| AppError::session(format!("传输任务 {group_id} 不存在")))
    }

    /// 查询传输组所属会话（resume/retry 重建 SFTP 客户端用）
    pub async fn group_session(&self, group_id: &str) -> Option<String> {
        self.groups
            .lock()
            .await
            .get(group_id)
            .map(|g| g.session_id.clone())
    }

    /// 暂停（进行中/排队 → 暂停，保留进度）
    pub async fn pause<R: Runtime>(&self, app: &AppHandle<R>, group_id: &str) -> Result<(), AppError> {
        let group = self.get_group(group_id).await?;
        let status = *group.status.lock().await;
        if !matches!(status, TransferStatus::Queued | TransferStatus::Running) {
            return Ok(());
        }
        // send_replace 无条件更新值（watch::send 在无接收者时不生效，
        // 而 Receiver 仅由 worker 按需 borrow，不长期持有）
        group.control_tx.send_replace(Ctrl::Pause);
        if let Some(token) = group.current_cancel.lock().await.as_ref() {
            token.cancel();
        }
        // worker 退出时会置 Paused 并 emit；此处兜底（worker 可能还在收尾）
        group.set_status(TransferStatus::Paused).await;
        emit_group(app, &group);
        Ok(())
    }

    /// 恢复（暂停 → 从断点继续）
    pub async fn resume<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        group_id: &str,
        client: SftpClient,
    ) -> Result<(), AppError> {
        let group = self.get_group(group_id).await?;
        let status = *group.status.lock().await;
        if status != TransferStatus::Paused {
            return Ok(());
        }
        group.control_tx.send_replace(Ctrl::Run);
        spawn_worker(app.clone(), self.semaphore.clone(), group, client).await;
        Ok(())
    }

    /// 重试（失败/已取消 → 从断点继续）
    pub async fn retry<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        group_id: &str,
        client: SftpClient,
    ) -> Result<(), AppError> {
        let group = self.get_group(group_id).await?;
        let status = *group.status.lock().await;
        if !matches!(status, TransferStatus::Failed | TransferStatus::Canceled) {
            return Ok(());
        }
        *group.error.lock().await = None;
        group.control_tx.send_replace(Ctrl::Run);
        spawn_worker(app.clone(), self.semaphore.clone(), group, client).await;
        Ok(())
    }

    /// 取消（终态，可 retry）
    pub async fn cancel<R: Runtime>(&self, app: &AppHandle<R>, group_id: &str) -> Result<(), AppError> {
        let group = self.get_group(group_id).await?;
        let status = *group.status.lock().await;
        if matches!(
            status,
            TransferStatus::Completed | TransferStatus::Canceled
        ) {
            return Ok(());
        }
        group.control_tx.send_replace(Ctrl::Cancel);
        if let Some(token) = group.current_cancel.lock().await.as_ref() {
            token.cancel();
        }
        group.set_status(TransferStatus::Canceled).await;
        emit_group(app, &group);
        Ok(())
    }

    /// 全部组快照（可按会话过滤）
    pub async fn snapshot(&self, session_id: Option<&str>) -> Vec<TransferEventPayload> {
        let groups = self.groups.lock().await;
        let mut list: Vec<TransferEventPayload> = groups
            .values()
            .filter(|g| session_id.map_or(true, |s| g.session_id == s))
            .map(|g| {
                g.refresh_aggregates();
                g.snapshot()
            })
            .collect();
        list.sort_by_key(|p| p.created_at_ms);
        list
    }

    /// 清除已完成/失败/已取消的组
    pub async fn clear_finished(&self, session_id: Option<&str>) {
        self.groups.lock().await.retain(|_, g| {
            let session_match = session_id.map_or(true, |s| g.session_id == s);
            if !session_match {
                return true;
            }
            // try_lock 失败说明 worker 正在推进（活跃），保守保留
            let finished = g
                .status
                .try_lock()
                .map(|s| {
                    matches!(
                        *s,
                        TransferStatus::Completed | TransferStatus::Failed | TransferStatus::Canceled
                    )
                })
                .unwrap_or(false);
            !finished
        });
    }

    /// 会话断开：取消该会话所有未完成传输
    pub async fn cancel_session<R: Runtime>(&self, app: Option<&AppHandle<R>>, session_id: &str) {
        let groups: Vec<Arc<TransferGroup>> = self
            .groups
            .lock()
            .await
            .values()
            .filter(|g| g.session_id == session_id)
            .cloned()
            .collect();
        for group in groups {
            let status = *group.status.lock().await;
            if matches!(
                status,
                TransferStatus::Completed | TransferStatus::Canceled
            ) {
                continue;
            }
            group.control_tx.send_replace(Ctrl::Cancel);
            if let Some(token) = group.current_cancel.lock().await.as_ref() {
                token.cancel();
            }
            group.set_status(TransferStatus::Canceled).await;
            *group.error.lock().await = Some("会话已断开".into());
            group.speed_bps.store(0, Ordering::Relaxed);
            if let Some(app) = app {
                emit_group(app, &group);
            }
        }
    }

    /// 应用退出：取消全部（无事件推送）
    pub async fn cancel_all(&self) {
        let groups: Vec<Arc<TransferGroup>> =
            self.groups.lock().await.values().cloned().collect();
        for group in groups {
            group.control_tx.send_replace(Ctrl::Cancel);
            if let Some(token) = group.current_cancel.lock().await.as_ref() {
                token.cancel();
            }
        }
    }
}

/// 组 worker：串行处理组内文件，驱动状态迁移与事件
async fn spawn_worker<R: Runtime>(
    app: AppHandle<R>,
    semaphore: Arc<Semaphore>,
    group: Arc<TransferGroup>,
    client: SftpClient,
) {
    let cancel = CancellationToken::new();
    // 注册当前运行令牌（pause/cancel 用）；同步等待注册完成，
    // 保证 pause/cancel 命令返回后一定能取消到本次运行
    *group.current_cancel.lock().await = Some(cancel.clone());
    tokio::spawn(async move {
        run_group(app, semaphore, group, client, cancel).await;
    });
}

async fn run_group<R: Runtime>(
    app: AppHandle<R>,
    semaphore: Arc<Semaphore>,
    group: Arc<TransferGroup>,
    client: SftpClient,
    cancel: CancellationToken,
) {
    // 等待并发许可（排队期间可被取消）
    let _permit = tokio::select! {
        _ = cancel.cancelled() => {
            finish_by_ctrl(&app, &group).await;
            return;
        }
        permit = semaphore.acquire_owned() => match permit {
            Ok(p) => p,
            Err(_) => return,
        },
    };

    // 纯空目录：建目录即完成（只含空子目录的情况走主路径，由补建循环创建）
    if group.files.is_empty() && group.empty_dirs.is_empty() {
        let result = match group.kind {
            TransferKind::Upload => match &group.remote_root {
                Some(root) if !root.is_empty() => client.mkdir_recursive(root).await,
                _ => Ok(()),
            },
            TransferKind::Download => {
                if let Some(base) = &group.local_base {
                    tokio::fs::create_dir_all(base).await.map_err(ssh_core::Error::from)
                } else {
                    Ok(())
                }
            }
        };
        group.refresh_aggregates();
        match result {
            Ok(()) => group.set_status(TransferStatus::Completed).await,
            Err(e) => {
                *group.error.lock().await = Some(e.to_string());
                group.set_status(TransferStatus::Failed).await;
            }
        }
        group.speed_bps.store(0, Ordering::Relaxed);
        emit_group(&app, &group);
        return;
    }

    group.set_status(TransferStatus::Running).await;
    emit_group(&app, &group);

    // 速度采样任务（每 250ms 推送进度；随 emitter_cancel 令牌退出）
    let emitter_cancel = CancellationToken::new();
    let _emitter = tokio::spawn(speed_emitter(
        app.clone(),
        group.clone(),
        emitter_cancel.clone(),
    ));

    for file in &group.files {
        // 每个文件开始前检查控制指令（watch guard 非 Send，先拷贝值再跨 await）
        let ctrl = *group.control_tx.borrow();
        match ctrl {
            Ctrl::Pause | Ctrl::Cancel => {
                emitter_cancel.cancel();
                finish_by_ctrl(&app, &group).await;
                return;
            }
            Ctrl::Run => {}
        }
        if file.done.load(Ordering::Relaxed) {
            continue;
        }

        let offset = file.bytes_done.load(Ordering::Relaxed);
        let result = match group.kind {
            TransferKind::Upload => {
                // 确保远端父目录存在（文件夹上传）
                let parent = remote_parent(&file.remote_path);
                if let Err(e) = client.mkdir_recursive(&parent).await {
                    Err(e)
                } else {
                    client
                        .upload_file(&file.local_path, &file.remote_path, offset, &cancel, &file.bytes_done)
                        .await
                }
            }
            TransferKind::Download => {
                // 确保本地父目录存在
                if let Some(parent) = file.local_path.parent() {
                    let _ = tokio::fs::create_dir_all(parent).await;
                }
                client
                    .download_file(&file.remote_path, &file.local_path, offset, &cancel, &file.bytes_done)
                    .await
            }
        };

        match result {
            Ok(()) => {
                file.bytes_done.store(file.size_total, Ordering::Relaxed);
                file.done.store(true, Ordering::Relaxed);
                group.refresh_aggregates();
                emit_group(&app, &group);
            }
            Err(ssh_core::Error::Canceled) => {
                emitter_cancel.cancel();
                group.speed_bps.store(0, Ordering::Relaxed);
                finish_by_ctrl(&app, &group).await;
                return;
            }
            Err(e) => {
                emitter_cancel.cancel();
                group.speed_bps.store(0, Ordering::Relaxed);
                *group.error.lock().await = Some(e.to_string());
                group.set_status(TransferStatus::Failed).await;
                group.refresh_aggregates();
                emit_group(&app, &group);
                return;
            }
        }
    }

    // 补建子树为空的目录（文件夹递归传输时保留空目录结构；失败视为组失败）
    for d in &group.empty_dirs {
        let result = match d {
            EmptyDir::Remote(p) => client.mkdir_recursive(p).await,
            EmptyDir::Local(p) => {
                tokio::fs::create_dir_all(p).await.map_err(ssh_core::Error::from)
            }
        };
        if let Err(e) = result {
            emitter_cancel.cancel();
            group.speed_bps.store(0, Ordering::Relaxed);
            *group.error.lock().await = Some(e.to_string());
            group.set_status(TransferStatus::Failed).await;
            group.refresh_aggregates();
            emit_group(&app, &group);
            return;
        }
    }

    emitter_cancel.cancel();
    group.speed_bps.store(0, Ordering::Relaxed);
    group.set_status(TransferStatus::Completed).await;
    group.refresh_aggregates();
    emit_group(&app, &group);
}

/// 按当前控制指令收尾（Pause → Paused；Cancel → Canceled）
async fn finish_by_ctrl<R: Runtime>(app: &AppHandle<R>, group: &TransferGroup) {
    let ctrl = *group.control_tx.borrow();
    match ctrl {
        Ctrl::Cancel => group.set_status(TransferStatus::Canceled).await,
        _ => group.set_status(TransferStatus::Paused).await,
    }
    group.speed_bps.store(0, Ordering::Relaxed);
    group.refresh_aggregates();
    emit_group(app, group);
}

/// 周期性推送进度与速度（EMA 平滑）
async fn speed_emitter<R: Runtime>(app: AppHandle<R>, group: Arc<TransferGroup>, cancel: CancellationToken) {
    let mut last_bytes = group.bytes_done.load(Ordering::Relaxed);
    let mut last_at = tokio::time::Instant::now();
    let mut interval = tokio::time::interval(Duration::from_millis(250));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            _ = cancel.cancelled() => return,
            _ = interval.tick() => {
                let cur = group.bytes_done.load(Ordering::Relaxed);
                let now = tokio::time::Instant::now();
                let dt = now.duration_since(last_at).as_secs_f64();
                if dt > 0.0 {
                    let inst = (cur.saturating_sub(last_bytes)) as f64 / dt;
                    let prev = group.speed_bps.load(Ordering::Relaxed) as f64;
                    let smoothed = if prev == 0.0 { inst } else { prev * 0.5 + inst * 0.5 };
                    group.speed_bps.store(smoothed as u64, Ordering::Relaxed);
                    last_bytes = cur;
                    last_at = now;
                    emit_group(&app, &group);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_join_and_parent() {
        assert_eq!(remote_join("/home", "a.txt"), "/home/a.txt");
        assert_eq!(remote_join("/home/", "a.txt"), "/home/a.txt");
        assert_eq!(remote_parent("/home/a.txt"), "/home");
        assert_eq!(remote_parent("/a.txt"), "/");
    }
}
