//! # SQLite 配置存储
//!
//! 阶段 2 实现：sessions/known_hosts/keys/tunnels/transfer_tasks 表。
//! 当前为骨架占位。

use std::path::PathBuf;
use std::sync::Arc;

use rusqlite::Connection;
use tokio::sync::Mutex;

/// SQLite 存储句柄
pub struct SqliteStorage {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteStorage {
    /// 打开数据库（不存在则创建）
    pub fn open(db_path: PathBuf) -> anyhow::Result<Self> {
        let conn = Connection::open(db_path)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// 打开内存数据库（测试用）
    #[cfg(test)]
    pub fn open_in_memory() -> anyhow::Result<Self> {
        let conn = Connection::open_in_memory()?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }
}
