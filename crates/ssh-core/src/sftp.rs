//! # SFTP 文件传输客户端
//!
//! 基于 `russh-sftp` 在已有 SSH 连接上打开独立 SFTP 通道，
//! 提供目录浏览 / 上传 / 下载 / 增删改能力。
//!
//! 用法：`SshSession::open_sftp()` 建立客户端（每会话可缓存复用，
//! [`SftpClient`] 内部为 `Arc`，`clone` 共享同一通道）。

use serde::{Deserialize, Serialize};

use crate::{Error, Result};

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

/// SFTP 客户端（Arc 共享，clone 复用同一底层通道）
#[derive(Clone)]
pub struct SftpClient {
    session: std::sync::Arc<russh_sftp::client::SftpSession>,
}

impl SftpClient {
    /// 由已初始化的 SftpSession 构造（Arc 共享）
    pub fn new(session: russh_sftp::client::SftpSession) -> Self {
        Self {
            session: std::sync::Arc::new(session),
        }
    }

    async fn sftp(&self) -> &russh_sftp::client::SftpSession {
        &self.session
    }

    /// 读取目录（目录优先、名称字母序）
    pub async fn list(&self, path: &str) -> Result<Vec<SftpEntry>> {
        let read_dir = self
            .sftp()
            .await
            .read_dir(path.to_string())
            .await
            .map_err(|e| Error::Terminal(format!("读取目录 {path} 失败: {e}")))?;

        let mut entries: Vec<SftpEntry> = read_dir
            .map(|entry| {
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
            })
            .collect();

        entries.sort_by(|a, b| {
            b.is_dir
                .cmp(&a.is_dir)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        Ok(entries)
    }

    /// 解析远端 home 目录（用于初始路径）
    pub async fn home(&self) -> Result<String> {
        self.sftp()
            .await
            .canonicalize(".")
            .await
            .map_err(|e| Error::Terminal(format!("解析远端 home 失败: {e}")))
    }

    /// 下载整个远端文件到内存（小文件；大文件后续换流式）
    pub async fn read(&self, path: &str) -> Result<Vec<u8>> {
        self.sftp()
            .await
            .read(path.to_string())
            .await
            .map_err(|e| Error::Terminal(format!("读取文件 {path} 失败: {e}")))
    }

    /// 上传字节写入远端文件（覆盖）
    pub async fn write(&self, path: &str, data: Vec<u8>) -> Result<()> {
        self.sftp()
            .await
            .write(path.to_string(), &data)
            .await
            .map_err(|e| Error::Terminal(format!("写入文件 {path} 失败: {e}")))
    }

    /// 创建目录
    pub async fn mkdir(&self, path: &str) -> Result<()> {
        self.sftp()
            .await
            .create_dir(path.to_string())
            .await
            .map_err(|e| Error::Terminal(format!("创建目录 {path} 失败: {e}")))
    }

    /// 删除文件
    pub async fn remove_file(&self, path: &str) -> Result<()> {
        self.sftp()
            .await
            .remove_file(path.to_string())
            .await
            .map_err(|e| Error::Terminal(format!("删除文件 {path} 失败: {e}")))
    }

    /// 删除空目录
    pub async fn remove_dir(&self, path: &str) -> Result<()> {
        self.sftp()
            .await
            .remove_dir(path.to_string())
            .await
            .map_err(|e| Error::Terminal(format!("删除目录 {path} 失败: {e}")))
    }

    /// 重命名 / 移动
    pub async fn rename(&self, old_path: &str, new_path: &str) -> Result<()> {
        self.sftp()
            .await
            .rename(old_path.to_string(), new_path.to_string())
            .await
            .map_err(|e| Error::Terminal(format!("重命名 {old_path} → {new_path} 失败: {e}")))
    }
}
