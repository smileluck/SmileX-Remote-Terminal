//! # SFTP 文件传输客户端
//!
//! 基于 `russh-sftp` 在已有 SSH 连接上打开独立 SFTP 通道，
//! 提供目录浏览 / 流式上传下载（分块、可取消、可续传）/ 增删改能力。
//!
//! 用法：`SshSession::open_sftp()` 建立客户端（每会话可缓存复用，
//! [`SftpClient`] 内部为 `Arc`，`clone` 共享同一通道）。
//!
//! 大文件传输走 [`SftpClient::upload_file`] / [`SftpClient::download_file`]：
//! 数据在 Rust 侧分块搬运、不进内存整体缓冲，进度通过 `AtomicU64` 累计，
//! 取消/暂停通过 `CancellationToken` 触发。
//!
//! ## 连接轮换
//!
//! russh 0.45 的会话循环在单条 SSH 连接累计约 1GiB 传输量后直接退出
//! （在途请求悬挂无错误无超时，后续操作报 session closed）。因此
//! [`SftpClient`] 统计累计流量，达到 [`CHANNEL_ROTATE_BYTES`] 即在块边界
//! 用保存的连接配置透明重建 SSH 连接 + SFTP 通道，并按当前偏移重开
//! 远端句柄，调用方无感知。

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Weak};
use std::time::Duration;

use russh_sftp::client::fs::{DirEntry, File};
use russh_sftp::client::SftpSession;
use russh_sftp::protocol::OpenFlags;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

use crate::connection::SshSession;
use crate::{Error, Result};

/// 流式传输分块大小（SFTP v3 单包上限 32KB）
const CHUNK_SIZE: usize = 32 * 1024;

/// 累计流量轮换阈值（russh 0.45 单连接在 ~1GiB 会话循环退出，留一半余量）
const CHANNEL_ROTATE_BYTES: u64 = 512 * 1024 * 1024;

/// 远端目录条目（序列化给前端）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SftpEntry {
    /// 文件名
    pub name: String,
    /// 完整路径（父目录 + 文件名）
    pub path: String,
    /// 是否目录
    pub is_dir: bool,
    /// 是否符号链接
    pub is_symlink: bool,
    /// 文件大小（字节；目录为 0）
    pub size: u64,
    /// 修改时间（Unix 秒）
    pub mtime: Option<u64>,
}

/// 由 DirEntry 构建 SftpEntry
fn to_entry(entry: &DirEntry) -> SftpEntry {
    let ft = entry.file_type();
    let meta = entry.metadata();
    SftpEntry {
        name: entry.file_name(),
        path: entry.path(),
        is_dir: ft.is_dir(),
        is_symlink: ft.is_symlink(),
        size: meta.size.unwrap_or(0),
        mtime: meta.mtime.map(|v| v as u64),
    }
}

/// SFTP 客户端（Arc 共享，clone 复用；内部通道可轮换）
#[derive(Clone)]
pub struct SftpClient {
    /// 当前 SFTP 通道（轮换时整体替换；读多写少用 RwLock）
    session: Arc<RwLock<Arc<SftpSession>>>,
    /// 所属 SSH 会话弱引用（开新通道用；会话已释放则不再轮换）
    owner: Weak<SshSession>,
    /// 当前连接累计流过字节（上传 + 下载），达阈值轮换
    channel_bytes: Arc<AtomicU64>,
}

impl SftpClient {
    /// 由 SftpSession + 所属会话构造（通常经 [`SshSession::open_sftp`]）
    pub fn new(session: SftpSession, owner: Weak<SshSession>) -> Self {
        Self {
            session: Arc::new(RwLock::new(Arc::new(session))),
            owner,
            channel_bytes: Arc::new(AtomicU64::new(0)),
        }
    }

    /// 当前通道快照（方法内短时使用；跨轮换仍持有旧通道直至操作结束）
    async fn sess(&self) -> Arc<SftpSession> {
        self.session.read().await.clone()
    }

    /// 累计流量达阈值则轮换，返回 `true` 表示已轮换
    /// （调用方需在新通道上重开文件句柄）
    ///
    /// russh 0.45 的会话循环在单条 SSH 连接累计约 1GiB 传输量后直接退出
    /// （在途请求悬挂、后续操作报 session closed，同连接开新通道也无效），
    /// 因此这里整连接重建：用保存的连接配置新建 SshSession + SFTP 通道。
    async fn rotate_due(&self) -> Result<bool> {
        if self.channel_bytes.load(Ordering::Relaxed) < CHANNEL_ROTATE_BYTES {
            return Ok(false);
        }
        // 并发共享同一 client 时，另一 worker 可能刚轮换过（计数已清零）
        let Some(owner) = self.owner.upgrade() else {
            // 会话已释放，无法轮换，让旧通道继续（通常也快结束了）
            return Ok(false);
        };
        let (config, known_hosts) = &owner.reconnect;
        let rotate = async {
            let fresh = SshSession::connect(config, known_hosts.clone(), None).await?;
            fresh.open_sftp_channel().await
        };
        let fresh = tokio::time::timeout(Duration::from_secs(15), rotate)
            .await
            .map_err(|_| Error::Terminal("SFTP 连接轮换超时".into()))??;
        *self.session.write().await = Arc::new(fresh);
        self.channel_bytes.store(0, Ordering::Relaxed);
        tracing::info!(
            "SFTP 已轮换到新 SSH 连接（单连接累计超过 {} 字节）",
            CHANNEL_ROTATE_BYTES
        );
        Ok(true)
    }

    /// 读取目录（目录优先、名称字母序）
    pub async fn list(&self, path: &str) -> Result<Vec<SftpEntry>> {
        let sftp = self.sess().await;
        let read_dir = sftp
            .read_dir(path.to_string())
            .await
            .map_err(|e| Error::Terminal(format!("读取目录 {path} 失败: {e}")))?;

        let mut entries: Vec<SftpEntry> = read_dir.map(|entry| to_entry(&entry)).collect();

        entries.sort_by(|a, b| {
            b.is_dir
                .cmp(&a.is_dir)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        Ok(entries)
    }

    /// 解析远端 home 目录（用于初始路径）
    pub async fn home(&self) -> Result<String> {
        self.sess()
            .await
            .canonicalize(".")
            .await
            .map_err(|e| Error::Terminal(format!("解析远端 home 失败: {e}")))
    }

    /// 查询远端文件/目录元数据（is_symlink 基于 lstat；is_dir 跟随符号链接）
    pub async fn stat(&self, path: &str) -> Result<SftpEntry> {
        let sftp = self.sess().await;
        let lstat = sftp
            .symlink_metadata(path.to_string())
            .await
            .map_err(|e| Error::Terminal(format!("查询 {path} 失败: {e}")))?;
        let name = path.rsplit('/').next().unwrap_or(path).to_string();
        if lstat.is_symlink() {
            // 符号链接：按文件对待（下载时读取目标内容），目录与否不展开
            return Ok(SftpEntry {
                name,
                path: path.to_string(),
                is_dir: false,
                is_symlink: true,
                size: 0,
                mtime: lstat.mtime.map(|v| v as u64),
            });
        }
        let attrs = sftp
            .metadata(path.to_string())
            .await
            .map_err(|e| Error::Terminal(format!("查询 {path} 失败: {e}")))?;
        Ok(SftpEntry {
            name,
            path: path.to_string(),
            is_dir: attrs.is_dir(),
            is_symlink: false,
            size: attrs.size.unwrap_or(0),
            mtime: attrs.mtime.map(|v| v as u64),
        })
    }

    /// 递归列举目录下所有文件（跳过符号链接条目，防止环）
    ///
    /// 迭代式 DFS（显式栈），避免深目录下 async 递归的 Box 开销与栈溢出。
    pub async fn walk_files(&self, root: &str) -> Result<Vec<SftpEntry>> {
        self.walk_tree(root).await.map(|(files, _)| files)
    }

    /// 递归列举目录下所有文件 + 子树无文件的“空目录”（下载时需显式创建）
    pub async fn walk_tree(&self, root: &str) -> Result<(Vec<SftpEntry>, Vec<String>)> {
        let sftp = self.sess().await;
        let mut files = Vec::new();
        let mut dirs = Vec::new();
        let mut stack = vec![root.to_string()];
        while let Some(dir) = stack.pop() {
            let read_dir = sftp
                .read_dir(dir.clone())
                .await
                .map_err(|e| Error::Terminal(format!("读取目录 {dir} 失败: {e}")))?;
            for entry in read_dir {
                let ft = entry.file_type();
                if ft.is_symlink() {
                    continue;
                }
                if ft.is_dir() {
                    dirs.push(entry.path());
                    stack.push(entry.path());
                } else {
                    files.push(to_entry(&entry));
                }
            }
        }
        let empty_dirs = dirs
            .into_iter()
            .filter(|d| !files.iter().any(|f| f.path.starts_with(d)))
            .collect();
        Ok((files, empty_dirs))
    }

    /// 流式上传：本地文件 → 远端路径
    ///
    /// - `offset > 0` 时续传（远端文件不截断，本地与远端均 seek 到 offset）
    /// - `progress` 为累计已传字节（含 offset），调用方轮询计算速度
    /// - 连接累计流量达阈值时在块边界轮换连接并重开远端句柄
    pub async fn upload_file(
        &self,
        local_path: &Path,
        remote_path: &str,
        offset: u64,
        cancel: &CancellationToken,
        progress: &AtomicU64,
    ) -> Result<()> {
        let mut pos = offset;
        let mut remote = open_remote_write(&*self.sess().await, remote_path, offset).await?;
        let mut local = tokio::fs::File::open(local_path).await?;
        if offset > 0 {
            local.seek(std::io::SeekFrom::Start(offset)).await?;
        }

        let mut buf = vec![0u8; CHUNK_SIZE];
        progress.store(offset, Ordering::Relaxed);
        loop {
            tokio::select! {
                _ = cancel.cancelled() => return Err(Error::Canceled),
                n = local.read(&mut buf) => {
                    let n = n?;
                    if n == 0 {
                        break;
                    }
                    remote
                        .write_all(&buf[..n])
                        .await
                        .map_err(|e| Error::Terminal(format!("写入远端文件失败: {e}")))?;
                    progress.fetch_add(n as u64, Ordering::Relaxed);
                    self.channel_bytes.fetch_add(n as u64, Ordering::Relaxed);
                    pos += n as u64;
                    if self.rotate_due().await? {
                        // 句柄绑定旧通道：drop 触发 close_nowait（同通道按序处理，
                        // 在途写入先于 close 落盘），再在新通道按当前偏移重开
                        drop(remote);
                        remote = open_remote_write(&*self.sess().await, remote_path, pos).await?;
                    }
                }
            }
        }
        // File 的 Drop 只发 close 不等回复，必须显式 close 确认写入落地
        remote
            .close()
            .await
            .map_err(|e| Error::Terminal(format!("关闭远端文件失败: {e}")))?;
        Ok(())
    }

    /// 流式下载：远端路径 → 本地文件
    ///
    /// - `offset > 0` 时续传（本地以 append 打开）
    /// - 连接累计流量达阈值时在块边界轮换连接并重开远端句柄
    pub async fn download_file(
        &self,
        remote_path: &str,
        local_path: &Path,
        offset: u64,
        cancel: &CancellationToken,
        progress: &AtomicU64,
    ) -> Result<()> {
        let mut pos = offset;
        let mut remote = open_remote_read(&*self.sess().await, remote_path, offset).await?;
        let mut opts = tokio::fs::OpenOptions::new();
        opts.write(true).create(true);
        if offset > 0 {
            opts.append(true);
        } else {
            opts.truncate(true);
        }
        let mut local = opts.open(local_path).await?;

        let mut buf = vec![0u8; CHUNK_SIZE];
        progress.store(offset, Ordering::Relaxed);
        loop {
            tokio::select! {
                _ = cancel.cancelled() => return Err(Error::Canceled),
                n = remote.read(&mut buf) => {
                    let n = n
                        .map_err(|e| Error::Terminal(format!("读取远端文件失败: {e}")))?;
                    if n == 0 {
                        break;
                    }
                    local.write_all(&buf[..n]).await?;
                    progress.fetch_add(n as u64, Ordering::Relaxed);
                    self.channel_bytes.fetch_add(n as u64, Ordering::Relaxed);
                    pos += n as u64;
                    if self.rotate_due().await? {
                        drop(remote);
                        remote = open_remote_read(&*self.sess().await, remote_path, pos).await?;
                    }
                }
            }
        }
        local.flush().await?;
        Ok(())
    }

    /// 创建目录
    pub async fn mkdir(&self, path: &str) -> Result<()> {
        self.sess()
            .await
            .create_dir(path.to_string())
            .await
            .map_err(|e| Error::Terminal(format!("创建目录 {path} 失败: {e}")))
    }

    /// 递归创建目录（mkdir -p 语义，已存在则忽略）
    pub async fn mkdir_recursive(&self, path: &str) -> Result<()> {
        let sftp = self.sess().await;
        let mut cur = String::new();
        for comp in path.split('/').filter(|c| !c.is_empty()) {
            cur = format!("{cur}/{comp}");
            if sftp.try_exists(cur.clone()).await.unwrap_or(false) {
                continue;
            }
            if let Err(e) = sftp.create_dir(cur.clone()).await {
                // 并发创建 / 已存在：再确认一次
                if sftp.try_exists(cur.clone()).await.unwrap_or(false) {
                    continue;
                }
                return Err(Error::Terminal(format!("创建目录 {cur} 失败: {e}")));
            }
        }
        Ok(())
    }

    /// 删除文件
    pub async fn remove_file(&self, path: &str) -> Result<()> {
        self.sess()
            .await
            .remove_file(path.to_string())
            .await
            .map_err(|e| Error::Terminal(format!("删除文件 {path} 失败: {e}")))
    }

    /// 删除空目录
    pub async fn remove_dir(&self, path: &str) -> Result<()> {
        self.sess()
            .await
            .remove_dir(path.to_string())
            .await
            .map_err(|e| Error::Terminal(format!("删除目录 {path} 失败: {e}")))
    }

    /// 递归删除文件/目录（符号链接只删链接本身）
    pub async fn remove_recursive(&self, path: &str) -> Result<()> {
        let sftp = self.sess().await;
        let lstat = sftp
            .symlink_metadata(path.to_string())
            .await
            .map_err(|e| Error::Terminal(format!("查询 {path} 失败: {e}")))?;
        if lstat.is_symlink() || !lstat.is_dir() {
            return self.remove_file(path).await;
        }
        // 迭代 DFS：先删所有文件，目录按发现顺序的逆序删（子目录先于父目录）
        let mut dirs_to_delete = vec![path.to_string()];
        let mut stack = vec![path.to_string()];
        while let Some(dir) = stack.pop() {
            let read_dir = sftp
                .read_dir(dir.clone())
                .await
                .map_err(|e| Error::Terminal(format!("读取目录 {dir} 失败: {e}")))?;
            for entry in read_dir {
                let ft = entry.file_type();
                if ft.is_dir() && !ft.is_symlink() {
                    stack.push(entry.path());
                    dirs_to_delete.push(entry.path());
                } else {
                    self.remove_file(&entry.path()).await?;
                }
            }
        }
        for dir in dirs_to_delete.iter().rev() {
            self.remove_dir(dir).await?;
        }
        Ok(())
    }

    /// 重命名 / 移动
    pub async fn rename(&self, old_path: &str, new_path: &str) -> Result<()> {
        self.sess()
            .await
            .rename(old_path.to_string(), new_path.to_string())
            .await
            .map_err(|e| Error::Terminal(format!("重命名 {old_path} → {new_path} 失败: {e}")))
    }
}

/// 按偏移打开远端文件供续写（不截断；offset>0 时 seek）
async fn open_remote_write(
    sftp: &SftpSession,
    remote_path: &str,
    offset: u64,
) -> Result<File> {
    let flags = if offset > 0 {
        OpenFlags::WRITE | OpenFlags::CREATE
    } else {
        OpenFlags::WRITE | OpenFlags::CREATE | OpenFlags::TRUNCATE
    };
    let mut remote = sftp
        .open_with_flags(remote_path.to_string(), flags)
        .await
        .map_err(|e| Error::Terminal(format!("打开远端文件 {remote_path} 失败: {e}")))?;
    if offset > 0 {
        remote
            .seek(std::io::SeekFrom::Start(offset))
            .await
            .map_err(|e| Error::Terminal(format!("定位远端文件偏移失败: {e}")))?;
    }
    Ok(remote)
}

/// 按偏移打开远端文件供读取（offset>0 时 seek）
async fn open_remote_read(sftp: &SftpSession, remote_path: &str, offset: u64) -> Result<File> {
    let mut remote = sftp
        .open(remote_path.to_string())
        .await
        .map_err(|e| Error::Terminal(format!("打开远端文件 {remote_path} 失败: {e}")))?;
    if offset > 0 {
        remote
            .seek(std::io::SeekFrom::Start(offset))
            .await
            .map_err(|e| Error::Terminal(format!("定位远端文件偏移失败: {e}")))?;
    }
    Ok(remote)
}
