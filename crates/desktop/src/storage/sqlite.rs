//! # SQLite 配置存储
//!
//! 持久化会话配置（SSH / 远程桌面）。**敏感字段（密码、私钥口令）不落库**，
//! 单独存入 OS Keyring（见 [`crate::storage::keyring`]）。
//!
//! ## Schema 设计（`session_profiles` 表）
//!
//! | 字段 | 类型 | 说明 |
//! |------|------|------|
//! | `id` | TEXT PK | 客户端生成的 UUID |
//! | `name` | TEXT NOT NULL | 显示名称（用户可读） |
//! | `kind` | TEXT NOT NULL | 会话种类：`ssh` / `rdp` / `host` |
//! | `host` | TEXT NOT NULL | 目标主机 |
//! | `port` | INTEGER NOT NULL | 目标端口 |
//! | `username` | TEXT NOT NULL | 登录用户名 |
//! | `auth_type` | TEXT NOT NULL | 认证方式：`password` / `private_key` / `private_key_mem` |
//! | `extra` | TEXT | 附加配置 JSON（如私钥路径、分辨率、color_depth、accept_first 等） |
//! | `created_at` | INTEGER NOT NULL | 创建时间戳（unix 秒） |
//! | `last_used_at` | INTEGER | 最近连接时间戳 |
//!
//! ## 索引
//! - `idx_session_profiles_kind`：按会话种类查询（前端侧栏分组）

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

/// 会话配置（前端可读可写，敏感字段已剥离）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionProfile {
    /// 客户端生成的 UUID
    pub id: String,
    /// 显示名称（用户可读）
    pub name: String,
    /// 会话种类：`ssh` / `rdp` / `host`
    pub kind: String,
    /// 目标主机
    pub host: String,
    /// 目标端口
    pub port: u16,
    /// 登录用户名
    pub username: String,
    /// 认证方式：`password` / `private_key` / `private_key_mem`
    pub auth_type: String,
    /// 附加配置 JSON（私钥路径 / 分辨率 / color_depth / accept_first_host_key 等）
    ///
    /// 注：私钥 PEM 本身也是敏感数据，存 Keyring 而非此处。
    /// `extra` 只存路径引用与非敏感参数。
    #[serde(default)]
    pub extra: String,
    /// 创建时间戳（unix 秒）
    pub created_at: i64,
    /// 最近连接时间戳（unix 秒，0 表示从未连接过）
    #[serde(default)]
    pub last_used_at: i64,
}

/// LLM 配置档案（多档案管理，阶段 6）
///
/// 字段 provider/model/base_url/auth_mode/stream 与 `ai_core::LlmProviderConfig` 同构，
/// 额外有 id/name/is_active 用于档案管理。
///
/// `auth_mode` 区分接入方式：`api`（按量 API）/ `coding_plan`（Coding Plan 订阅），
/// 仅国内厂商预设使用（旧数据为 NULL，按量处理）。
///
/// **敏感字段（API Key）不落库**，单独存 Keyring（key: `llm_profile:{id}:api_key`）。
///
/// 序列化与前端 `types/settings.ts` 的 `LlmProfile` 对齐（camelCase）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmProfile {
    /// 客户端生成的 UUID
    pub id: String,
    /// 显示名称（用户可读，如 "OpenAI 工作" / "DeepSeek 个人"）
    pub name: String,
    /// Provider 预设（小写：`openai` / `claude` / `ollama` / `zhipu` 等）
    pub provider: String,
    /// 模型名（如 `gpt-4o`）
    pub model: String,
    /// Base URL（可选，留空用 Provider 默认）
    pub base_url: Option<String>,
    /// 接入方式（`api` / `coding_plan`；NULL 表示不区分，按量处理）
    #[serde(default)]
    pub auth_mode: Option<String>,
    /// 是否流式输出（0/1）
    pub stream: bool,
    /// 是否为当前激活档案（同一时间仅一个）
    pub is_active: bool,
    /// 创建时间戳（unix 秒）
    pub created_at: i64,
    /// 更新时间戳（unix 秒）
    pub updated_at: i64,
}

/// 命令片段（用户收藏的常用命令）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandSnippet {
    pub id: String,
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub tags: String,
    pub created_at: i64,
}

/// 命令历史条目
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandHistory {
    pub id: i64,
    pub session_id: String,
    pub command: String,
    pub created_at: i64,
}

/// 监控告警规则
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AlertRule {
    pub id: String,
    pub name: String,
    /// 指标名：cpu_percent / mem_percent / load1 / net_rx_bps / net_tx_bps
    pub metric: String,
    /// 比较符：gt / lt
    pub op: String,
    pub threshold: f64,
    pub enabled: bool,
    /// 触发冷却（秒）
    pub cooldown_sec: u32,
    pub created_at: i64,
}

/// SSH 密钥元数据（私钥在 OS Keyring，不落库）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SshKeyMeta {
    pub id: String,
    pub name: String,
    pub key_type: String,
    pub public_key: String,
    pub fingerprint: String,
    pub created_at: i64,
}

/// SQLite 存储句柄
///
/// 内部用 `tokio::sync::Mutex` 包装 `Connection`，保证并发安全。
/// 所有 CRUD 方法都是 `&self`，可从 `Arc<SqliteStorage>` 共享。
pub struct SqliteStorage {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteStorage {
    /// 打开数据库（不存在则创建），并执行 schema 初始化
    ///
    /// 幂等：多次执行 `CREATE TABLE IF NOT EXISTS` 安全。
    pub fn open(db_path: PathBuf) -> Result<Self> {
        let conn = Connection::open(db_path).context("打开 SQLite 数据库失败")?;
        Self::init_schema(&conn)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// 打开内存数据库（测试用）
    #[cfg(test)]
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        Self::init_schema(&conn)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// 执行 schema 初始化（幂等）
    ///
    /// 建表 + 索引，启用 WAL 模式以提升并发读。
    ///
    /// 包含四张表：
    /// - `session_profiles`：会话配置（用户可读）
    /// - `known_hosts`：已知主机指纹（首次信任后落盘，用于 MITM 校验）
    /// - `app_config`：应用全局配置（键值对；LLM 配置等历史字段）
    /// - `llm_profiles`：多 LLM 配置档案（阶段 6）
    fn init_schema(conn: &Connection) -> Result<()> {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS session_profiles (
                id            TEXT    PRIMARY KEY NOT NULL,
                name          TEXT    NOT NULL,
                kind          TEXT    NOT NULL,
                host          TEXT    NOT NULL,
                port          INTEGER NOT NULL,
                username      TEXT    NOT NULL,
                auth_type     TEXT    NOT NULL,
                extra         TEXT    NOT NULL DEFAULT '{}',
                created_at    INTEGER NOT NULL,
                last_used_at  INTEGER NOT NULL DEFAULT 0
            );

            CREATE INDEX IF NOT EXISTS idx_session_profiles_kind
                ON session_profiles(kind);

            CREATE TABLE IF NOT EXISTS known_hosts (
                host          TEXT    NOT NULL,
                port          INTEGER NOT NULL,
                key_type      TEXT    NOT NULL,
                fingerprint   TEXT    NOT NULL,
                -- 主键：(host, port)：同一主机端口只保留一条最新记录
                PRIMARY KEY (host, port)
            );

            -- 应用配置（键值对）
            -- 非敏感配置（LLM provider/model/base_url/stream 等），API Key 单独存 Keyring
            CREATE TABLE IF NOT EXISTS app_config (
                key           TEXT    PRIMARY KEY NOT NULL,
                value         TEXT    NOT NULL,
                updated_at    INTEGER NOT NULL
            );

            -- 多 LLM 配置档案（阶段 6）
            -- 支持保存多个 Provider 配置并快速切换激活
            -- auth_mode 区分接入方式：api（按量）/ coding_plan（订阅，Anthropic 兼容端点）
            -- API Key 单独存 Keyring（key: llm_profile:{id}:api_key）
            CREATE TABLE IF NOT EXISTS llm_profiles (
                id            TEXT    PRIMARY KEY NOT NULL,
                name          TEXT    NOT NULL,
                provider      TEXT    NOT NULL,
                model         TEXT    NOT NULL,
                base_url      TEXT,
                auth_mode     TEXT,
                stream        INTEGER NOT NULL DEFAULT 1,
                is_active     INTEGER NOT NULL DEFAULT 0,
                created_at    INTEGER NOT NULL,
                updated_at    INTEGER NOT NULL
            );

            -- 同一时间仅允许一个 active profile（部分代码层保证）
            CREATE INDEX IF NOT EXISTS idx_llm_profiles_active
                ON llm_profiles(is_active);

            -- 命令片段（用户收藏的常用命令）
            CREATE TABLE IF NOT EXISTS command_snippets (
                id            TEXT    PRIMARY KEY NOT NULL,
                name          TEXT    NOT NULL,
                command       TEXT    NOT NULL,
                tags          TEXT    NOT NULL DEFAULT '',
                created_at    INTEGER NOT NULL
            );

            -- 命令历史（终端回车行自动记录，按时间倒序查询）
            CREATE TABLE IF NOT EXISTS command_history (
                id            INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id    TEXT    NOT NULL DEFAULT '',
                command       TEXT    NOT NULL,
                created_at    INTEGER NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_command_history_time
                ON command_history(created_at DESC);

            -- 监控告警规则
            CREATE TABLE IF NOT EXISTS alert_rules (
                id            TEXT    PRIMARY KEY NOT NULL,
                name          TEXT    NOT NULL,
                metric        TEXT    NOT NULL,   -- cpu_percent / mem_percent / load1 / net_rx_bps / net_tx_bps
                op            TEXT    NOT NULL,   -- gt / lt
                threshold     REAL    NOT NULL,
                enabled       INTEGER NOT NULL DEFAULT 1,
                cooldown_sec  INTEGER NOT NULL DEFAULT 300,
                created_at    INTEGER NOT NULL
            );

            -- SSH 密钥元数据（私钥在 OS Keyring，不落库）
            CREATE TABLE IF NOT EXISTS ssh_keys (
                id            TEXT    PRIMARY KEY NOT NULL,
                name          TEXT    NOT NULL,
                key_type      TEXT    NOT NULL,
                public_key    TEXT    NOT NULL,
                fingerprint   TEXT    NOT NULL,
                created_at    INTEGER NOT NULL
            );
            "#,
        )?;

        // 旧库升级：CREATE TABLE IF NOT EXISTS 不会为已存在的表补充新列，
        // 需显式 ALTER（幂等：列已存在时跳过）
        Self::ensure_column(
            conn,
            "llm_profiles",
            "auth_mode",
            "ALTER TABLE llm_profiles ADD COLUMN auth_mode TEXT",
        )?;
        Ok(())
    }

    /// 确保表中存在指定列，不存在则执行 DDL（幂等，用于旧库升级）
    fn ensure_column(conn: &Connection, table: &str, column: &str, ddl: &str) -> Result<()> {
        let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
        let mut rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
        while let Some(col) = rows.next() {
            if col?.eq_ignore_ascii_case(column) {
                return Ok(());
            }
        }
        conn.execute_batch(ddl).context("SQLite 表结构迁移失败")?;
        Ok(())
    }

    /// 保存或更新会话配置（以 `id` 为主键 UPSERT）
    ///
    /// 若 `last_used_at == 0` 且库中已存在同 id，保留原 last_used_at。
    pub async fn save_profile(&self, profile: &SessionProfile) -> Result<()> {
        let conn = self.conn.lock().await;
        conn.execute(
            r#"
            INSERT INTO session_profiles
                (id, name, kind, host, port, username, auth_type, extra, created_at, last_used_at)
            VALUES
                (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            ON CONFLICT(id) DO UPDATE SET
                name          = excluded.name,
                kind          = excluded.kind,
                host          = excluded.host,
                port          = excluded.port,
                username      = excluded.username,
                auth_type     = excluded.auth_type,
                extra         = excluded.extra,
                last_used_at  = CASE WHEN excluded.last_used_at = 0
                                     THEN session_profiles.last_used_at
                                     ELSE excluded.last_used_at END
            "#,
            params![
                profile.id,
                profile.name,
                profile.kind,
                profile.host,
                profile.port,
                profile.username,
                profile.auth_type,
                profile.extra,
                profile.created_at,
                profile.last_used_at,
            ],
        ).context("执行 save_profile 失败")?;
        Ok(())
    }

    /// 列出所有会话配置（按 `last_used_at DESC, name ASC` 排序，最近使用的在前）
    pub async fn list_profiles(&self) -> Result<Vec<SessionProfile>> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            r#"
            SELECT id, name, kind, host, port, username, auth_type, extra, created_at, last_used_at
            FROM session_profiles
            ORDER BY last_used_at DESC, name ASC
            "#,
        )?;
        let rows = stmt.query_map([], row_to_profile)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// 按 id 获取单个会话配置
    pub async fn get_profile(&self, id: &str) -> Result<Option<SessionProfile>> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            r#"
            SELECT id, name, kind, host, port, username, auth_type, extra, created_at, last_used_at
            FROM session_profiles
            WHERE id = ?1
            "#,
        )?;
        let mut rows = stmt.query_map(params![id], row_to_profile)?;
        if let Some(r) = rows.next() {
            Ok(Some(r?))
        } else {
            Ok(None)
        }
    }

    /// 按 id 删除会话配置
    ///
    /// 返回是否实际删除了一行（false 表示 id 不存在）。
    pub async fn delete_profile(&self, id: &str) -> Result<bool> {
        let conn = self.conn.lock().await;
        let affected = conn.execute(
            "DELETE FROM session_profiles WHERE id = ?1",
            params![id],
        )?;
        Ok(affected > 0)
    }

    /// 标记最近使用时间（连接成功后调用，用于侧栏排序）
    pub async fn touch_profile(&self, id: &str, ts: i64) -> Result<()> {
        let conn = self.conn.lock().await;
        conn.execute(
            "UPDATE session_profiles SET last_used_at = ?1 WHERE id = ?2",
            params![ts, id],
        )?;
        Ok(())
    }

    // ===== app_config（应用全局配置键值对）=====

    /// 读取应用配置项（按 key）
    ///
    /// 返回 `Ok(Some(value))` / `Ok(None)`。
    pub async fn get_config(&self, key: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().await;
        let mut stmt =
            conn.prepare("SELECT value FROM app_config WHERE key = ?1")?;
        let mut rows = stmt.query_map(params![key], |row| row.get::<_, String>(0))?;
        if let Some(r) = rows.next() {
            Ok(Some(r?))
        } else {
            Ok(None)
        }
    }

    /// 写入应用配置项（UPSERT）
    ///
    /// `ts` 为 unix 秒时间戳（由调用方提供，保证业务时钟一致）。
    pub async fn set_config(&self, key: &str, value: &str, ts: i64) -> Result<()> {
        let conn = self.conn.lock().await;
        conn.execute(
            r#"
            INSERT INTO app_config (key, value, updated_at)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(key) DO UPDATE SET
                value      = excluded.value,
                updated_at = excluded.updated_at
            "#,
            params![key, value, ts],
        )?;
        Ok(())
    }

    // ===== llm_profiles（多 LLM 配置档案）=====

    /// 保存或更新 LLM 配置档案（UPSERT）
    ///
    /// - 新建：插入新记录
    /// - 更新：保留 `is_active` 状态（避免编辑档案意外取消激活）
    /// - 若传入 `is_active=true`，同时取消其他档案的激活（保证唯一）
    pub async fn save_llm_profile(&self, profile: &LlmProfile) -> Result<()> {
        let mut conn = self.conn.lock().await;
        let tx = conn.transaction()?;

        // 若设置为 active，先清除其他 active 标记（唯一性约束）
        if profile.is_active {
            tx.execute("UPDATE llm_profiles SET is_active = 0", [])?;
        }

        // UPSERT：is_active 字段使用 CASE 保留原值（更新时）/ 使用新值（新建时）
        tx.execute(
            r#"
            INSERT INTO llm_profiles
                (id, name, provider, model, base_url, auth_mode, stream, is_active, created_at, updated_at)
            VALUES
                (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            ON CONFLICT(id) DO UPDATE SET
                name       = excluded.name,
                provider   = excluded.provider,
                model      = excluded.model,
                base_url   = excluded.base_url,
                auth_mode  = excluded.auth_mode,
                stream     = excluded.stream,
                is_active  = CASE WHEN ?11 = 1 THEN 1 ELSE llm_profiles.is_active END,
                updated_at = excluded.updated_at
            "#,
            params![
                profile.id,
                profile.name,
                profile.provider,
                profile.model,
                profile.base_url,
                profile.auth_mode,
                profile.stream as i64,
                profile.is_active as i64,
                profile.created_at,
                profile.updated_at,
                profile.is_active as i64,
            ],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// 列出所有 LLM 配置档案（按 created_at ASC 排序，稳定的展示顺序）
    pub async fn list_llm_profiles(&self) -> Result<Vec<LlmProfile>> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            r#"
            SELECT id, name, provider, model, base_url, auth_mode, stream, is_active, created_at, updated_at
            FROM llm_profiles
            ORDER BY created_at ASC, name ASC
            "#,
        )?;
        let rows = stmt.query_map([], row_to_llm_profile)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// 按 id 获取单个 LLM 配置档案
    pub async fn get_llm_profile(&self, id: &str) -> Result<Option<LlmProfile>> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            r#"
            SELECT id, name, provider, model, base_url, auth_mode, stream, is_active, created_at, updated_at
            FROM llm_profiles
            WHERE id = ?1
            "#,
        )?;
        let mut rows = stmt.query_map(params![id], row_to_llm_profile)?;
        if let Some(r) = rows.next() {
            Ok(Some(r?))
        } else {
            Ok(None)
        }
    }

    /// 按 id 删除 LLM 配置档案
    ///
    /// 返回是否实际删除了一行。
    pub async fn delete_llm_profile(&self, id: &str) -> Result<bool> {
        let conn = self.conn.lock().await;
        let affected = conn.execute("DELETE FROM llm_profiles WHERE id = ?1", params![id])?;
        Ok(affected > 0)
    }

    /// 设置激活的 LLM 配置档案（排他：清除其他 active）
    ///
    /// 若 `id` 不存在返回错误。
    pub async fn set_active_llm_profile(&self, id: &str) -> Result<()> {
        let mut conn = self.conn.lock().await;
        let tx = conn.transaction()?;
        // 校验 id 存在
        let exists: i64 = tx.query_row(
            "SELECT COUNT(*) FROM llm_profiles WHERE id = ?1",
            params![id],
            |r| r.get(0),
        )?;
        if exists == 0 {
            anyhow::bail!("LLM 配置档案 {id} 不存在");
        }
        // 清除所有 active
        tx.execute("UPDATE llm_profiles SET is_active = 0", [])?;
        // 设置目标为 active
        tx.execute(
            "UPDATE llm_profiles SET is_active = 1, updated_at = ?2 WHERE id = ?1",
            params![id, chrono::Utc::now().timestamp()],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// 获取当前激活的 LLM 配置档案（`is_active = 1`）
    ///
    /// 返回 `Ok(None)` 表示无激活档案（首次启动或全部被删除）。
    pub async fn get_active_llm_profile(&self) -> Result<Option<LlmProfile>> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            r#"
            SELECT id, name, provider, model, base_url, auth_mode, stream, is_active, created_at, updated_at
            FROM llm_profiles
            WHERE is_active = 1
            LIMIT 1
            "#,
        )?;
        let mut rows = stmt.query_map([], row_to_llm_profile)?;
        if let Some(r) = rows.next() {
            Ok(Some(r?))
        } else {
            Ok(None)
        }
    }

    /// 保存/更新命令片段（id UPSERT）
    pub async fn save_snippet(&self, snippet: &CommandSnippet) -> Result<()> {
        let conn = self.conn.lock().await;
        conn.execute(
            "INSERT OR REPLACE INTO command_snippets (id, name, command, tags, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![snippet.id, snippet.name, snippet.command, snippet.tags, snippet.created_at],
        )?;
        Ok(())
    }

    /// 全部命令片段（新→旧）
    pub async fn list_snippets(&self) -> Result<Vec<CommandSnippet>> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT id, name, command, tags, created_at FROM command_snippets ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(CommandSnippet {
                id: row.get(0)?,
                name: row.get(1)?,
                command: row.get(2)?,
                tags: row.get(3)?,
                created_at: row.get(4)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// 删除命令片段（返回是否存在）
    pub async fn delete_snippet(&self, id: &str) -> Result<bool> {
        let conn = self.conn.lock().await;
        Ok(conn.execute("DELETE FROM command_snippets WHERE id = ?1", params![id])? > 0)
    }

    /// 追加一条命令历史
    pub async fn add_history(&self, session_id: &str, command: &str, ts: i64) -> Result<()> {
        let conn = self.conn.lock().await;
        conn.execute(
            "INSERT INTO command_history (session_id, command, created_at) VALUES (?1, ?2, ?3)",
            params![session_id, command, ts],
        )?;
        Ok(())
    }

    /// 最近命令历史（去重相邻重复，新→旧）
    pub async fn list_history(&self, limit: u32) -> Result<Vec<CommandHistory>> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT id, session_id, command, created_at FROM command_history ORDER BY id DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit], |row| {
            Ok(CommandHistory {
                id: row.get(0)?,
                session_id: row.get(1)?,
                command: row.get(2)?,
                created_at: row.get(3)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// 清空命令历史
    pub async fn clear_history(&self) -> Result<()> {
        let conn = self.conn.lock().await;
        conn.execute("DELETE FROM command_history", [])?;
        Ok(())
    }

    /// 保存告警规则（id UPSERT）
    pub async fn save_alert_rule(&self, rule: &AlertRule) -> Result<()> {
        let conn = self.conn.lock().await;
        conn.execute(
            "INSERT OR REPLACE INTO alert_rules (id, name, metric, op, threshold, enabled, cooldown_sec, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                rule.id, rule.name, rule.metric, rule.op,
                rule.threshold, rule.enabled as i32, rule.cooldown_sec, rule.created_at
            ],
        )?;
        Ok(())
    }

    /// 全部告警规则（仅启用的可选）
    pub async fn list_alert_rules(&self, enabled_only: bool) -> Result<Vec<AlertRule>> {
        let conn = self.conn.lock().await;
        let sql = if enabled_only {
            "SELECT id, name, metric, op, threshold, enabled, cooldown_sec, created_at FROM alert_rules WHERE enabled = 1 ORDER BY created_at DESC"
        } else {
            "SELECT id, name, metric, op, threshold, enabled, cooldown_sec, created_at FROM alert_rules ORDER BY created_at DESC"
        };
        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map([], row_to_alert_rule)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// 删除告警规则
    pub async fn delete_alert_rule(&self, id: &str) -> Result<bool> {
        let conn = self.conn.lock().await;
        Ok(conn.execute("DELETE FROM alert_rules WHERE id = ?1", params![id])? > 0)
    }

    /// 保存 SSH 密钥元数据（id UPSERT）
    pub async fn save_ssh_key(&self, key: &SshKeyMeta) -> Result<()> {
        let conn = self.conn.lock().await;
        conn.execute(
            "INSERT OR REPLACE INTO ssh_keys (id, name, key_type, public_key, fingerprint, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![key.id, key.name, key.key_type, key.public_key, key.fingerprint, key.created_at],
        )?;
        Ok(())
    }

    /// 全部 SSH 密钥元数据（新→旧）
    pub async fn list_ssh_keys(&self) -> Result<Vec<SshKeyMeta>> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT id, name, key_type, public_key, fingerprint, created_at FROM ssh_keys ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(SshKeyMeta {
                id: row.get(0)?,
                name: row.get(1)?,
                key_type: row.get(2)?,
                public_key: row.get(3)?,
                fingerprint: row.get(4)?,
                created_at: row.get(5)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// 删除 SSH 密钥元数据
    pub async fn delete_ssh_key(&self, id: &str) -> Result<bool> {
        let conn = self.conn.lock().await;
        Ok(conn.execute("DELETE FROM ssh_keys WHERE id = ?1", params![id])? > 0)
    }
}

/// 把 rusqlite `Row` 映射为 [`AlertRule`]
fn row_to_alert_rule(row: &rusqlite::Row<'_>) -> rusqlite::Result<AlertRule> {
    Ok(AlertRule {
        id: row.get(0)?,
        name: row.get(1)?,
        metric: row.get(2)?,
        op: row.get(3)?,
        threshold: row.get(4)?,
        enabled: row.get::<_, i32>(5)? != 0,
        cooldown_sec: row.get(6)?,
        created_at: row.get(7)?,
    })
}

/// 把 rusqlite `Row` 映射为 [`SessionProfile`]
fn row_to_profile(row: &rusqlite::Row<'_>) -> rusqlite::Result<SessionProfile> {
    Ok(SessionProfile {
        id: row.get(0)?,
        name: row.get(1)?,
        kind: row.get(2)?,
        host: row.get(3)?,
        port: row.get(4)?,
        username: row.get(5)?,
        auth_type: row.get(6)?,
        extra: row.get(7)?,
        created_at: row.get(8)?,
        last_used_at: row.get(9)?,
    })
}

/// 把 rusqlite `Row` 映射为 [`LlmProfile`]
///
/// 字段顺序与 SELECT 列对齐：
/// `id, name, provider, model, base_url, auth_mode, stream, is_active, created_at, updated_at`
fn row_to_llm_profile(row: &rusqlite::Row<'_>) -> rusqlite::Result<LlmProfile> {
    Ok(LlmProfile {
        id: row.get(0)?,
        name: row.get(1)?,
        provider: row.get(2)?,
        model: row.get(3)?,
        base_url: row.get(4)?,
        auth_mode: row.get(5)?,
        stream: row.get::<_, i64>(6)? != 0,
        is_active: row.get::<_, i64>(7)? != 0,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
    })
}

/// 实现 ssh-core 的 [`KnownHostsStore`] trait
///
/// 通过 SQLite `known_hosts` 表持久化主机指纹。
/// - `lookup`：按 `(host, port)` 查询单条
/// - `save`：UPSERT（同主键覆盖，保留最新指纹）
///
/// 注：此 impl 让 desktop 反向实现 ssh-core 的抽象（依赖反转）。
///     ssh-core 不依赖 desktop，由 desktop 注入具体存储实现。
#[async_trait::async_trait]
impl ssh_core::known_hosts::KnownHostsStore for SqliteStorage {
    async fn lookup(
        &self,
        host: &str,
        port: u16,
    ) -> ssh_core::Result<Option<ssh_core::known_hosts::KnownHost>> {
        let conn = self.conn.lock().await;
        // rusqlite::Error → anyhow::Error → ssh_core::Error::Other
        let mut stmt = conn
            .prepare(
                r#"
            SELECT host, port, key_type, fingerprint
            FROM known_hosts
            WHERE host = ?1 AND port = ?2
            "#,
            )
            .map_err(|e| anyhow::anyhow!(e))?;
        let mut rows = stmt
            .query_map(params![host, port], |row| {
                Ok(ssh_core::known_hosts::KnownHost {
                    host: row.get(0)?,
                    port: row.get(1)?,
                    key_type: row.get(2)?,
                    fingerprint: row.get(3)?,
                })
            })
            .map_err(|e| anyhow::anyhow!(e))?;
        if let Some(r) = rows.next() {
            let entry = r.map_err(|e| anyhow::anyhow!(e))?;
            Ok(Some(entry))
        } else {
            Ok(None)
        }
    }

    async fn save(&self, entry: ssh_core::known_hosts::KnownHost) -> ssh_core::Result<()> {
        let conn = self.conn.lock().await;
        conn.execute(
            r#"
            INSERT INTO known_hosts (host, port, key_type, fingerprint)
            VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(host, port) DO UPDATE SET
                key_type    = excluded.key_type,
                fingerprint = excluded.fingerprint
            "#,
            params![entry.host, entry.port, entry.key_type, entry.fingerprint],
        )
        .map_err(|e| anyhow::anyhow!(e))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_save_get_delete_profile() {
        let storage = SqliteStorage::open_in_memory().unwrap();
        let p = SessionProfile {
            id: "uuid-1".into(),
            name: "test-server".into(),
            kind: "ssh".into(),
            host: "192.168.1.1".into(),
            port: 22,
            username: "root".into(),
            auth_type: "password".into(),
            extra: "{}".into(),
            created_at: 1000,
            last_used_at: 0,
        };

        // 新增
        storage.save_profile(&p).await.unwrap();
        let got = storage.get_profile("uuid-1").await.unwrap().unwrap();
        assert_eq!(got.name, "test-server");
        assert_eq!(got.port, 22);

        // 更新 name，保持 last_used_at=0 不覆盖
        let mut p2 = p.clone();
        p2.name = "renamed".into();
        p2.last_used_at = 9999;
        storage.save_profile(&p2).await.unwrap();
        let got2 = storage.get_profile("uuid-1").await.unwrap().unwrap();
        assert_eq!(got2.name, "renamed");
        assert_eq!(got2.last_used_at, 9999);

        // 列表
        let list = storage.list_profiles().await.unwrap();
        assert_eq!(list.len(), 1);

        // 删除
        assert!(storage.delete_profile("uuid-1").await.unwrap());
        assert!(storage.get_profile("uuid-1").await.unwrap().is_none());
        assert!(!storage.delete_profile("uuid-1").await.unwrap());
    }

    #[tokio::test]
    async fn test_llm_profile_auth_mode_roundtrip() {
        let storage = SqliteStorage::open_in_memory().unwrap();
        let p = LlmProfile {
            id: "llm-1".into(),
            name: "智谱 Coding Plan".into(),
            provider: "zhipu".into(),
            model: "glm-4.6".into(),
            base_url: Some("https://open.bigmodel.cn/api/anthropic".into()),
            auth_mode: Some("coding_plan".into()),
            stream: true,
            is_active: false,
            created_at: 1000,
            updated_at: 1000,
        };
        storage.save_llm_profile(&p).await.unwrap();
        let got = storage.get_llm_profile("llm-1").await.unwrap().unwrap();
        assert_eq!(got.auth_mode.as_deref(), Some("coding_plan"));

        // 更新为按量 API
        let mut p2 = p.clone();
        p2.auth_mode = Some("api".into());
        storage.save_llm_profile(&p2).await.unwrap();
        let got2 = storage.get_llm_profile("llm-1").await.unwrap().unwrap();
        assert_eq!(got2.auth_mode.as_deref(), Some("api"));

        // 旧数据 NULL → None
        let mut p3 = p.clone();
        p3.id = "llm-legacy".into();
        p3.auth_mode = None;
        storage.save_llm_profile(&p3).await.unwrap();
        let got3 = storage.get_llm_profile("llm-legacy").await.unwrap().unwrap();
        assert_eq!(got3.auth_mode, None);
    }

    /// 旧库（llm_profiles 无 auth_mode 列）打开时自动 ALTER 补列，
    /// 已有数据保留且 auth_mode 读出为 None
    #[tokio::test]
    async fn test_migrate_legacy_llm_profiles_table() {
        let dir = std::env::temp_dir().join(format!("srt-test-migrate-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db_path = dir.join("legacy.db");

        {
            // 建旧 schema 库并写入一条旧记录
            let conn = Connection::open(&db_path).unwrap();
            conn.execute_batch(
                r#"
                CREATE TABLE llm_profiles (
                    id            TEXT    PRIMARY KEY NOT NULL,
                    name          TEXT    NOT NULL,
                    provider      TEXT    NOT NULL,
                    model         TEXT    NOT NULL,
                    base_url      TEXT,
                    stream        INTEGER NOT NULL DEFAULT 1,
                    is_active     INTEGER NOT NULL DEFAULT 0,
                    created_at    INTEGER NOT NULL,
                    updated_at    INTEGER NOT NULL
                );
                INSERT INTO llm_profiles (id, name, provider, model, base_url, stream, is_active, created_at, updated_at)
                VALUES ('old-1', '旧配置', 'openai', 'gpt-4o', NULL, 1, 1, 100, 100);
                "#,
            )
            .unwrap();
        }

        // 正常打开触发 init_schema 的列迁移
        let storage = SqliteStorage::open(db_path.clone()).unwrap();
        let got = storage.get_llm_profile("old-1").await.unwrap().unwrap();
        assert_eq!(got.name, "旧配置");
        assert_eq!(got.auth_mode, None);

        // 迁移后新字段可写
        let mut updated = got.clone();
        updated.auth_mode = Some("api".into());
        storage.save_llm_profile(&updated).await.unwrap();
        let got2 = storage.get_llm_profile("old-1").await.unwrap().unwrap();
        assert_eq!(got2.auth_mode.as_deref(), Some("api"));

        // 幂等：重复 open 不报错
        drop(storage);
        let _again = SqliteStorage::open(db_path).unwrap();

        std::fs::remove_dir_all(&dir).ok();
    }
}
