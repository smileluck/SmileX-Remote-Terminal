//! # known_hosts 主机密钥校验
//!
//! 提供 host key 持久化与校验框架，防止中间人攻击（MITM）。
//!
//! ## 校验策略（`HostKeyPolicy`）
//!
//! | 策略 | 已知匹配 | 已知不匹配 | 未知 | 推荐场景 |
//! |------|---------|------------|------|----------|
//! | `Strict`     | ✅ 接受 | ❌ 拒绝 | ❌ 拒绝 | 生产环境 |
//! | `AcceptNew`  | ✅ 接受 | ❌ 拒绝 | ✅ 接受并保存 | 个人开发机 |
//! | `AcceptAll`  | ✅ 接受 | ✅ 接受 | ✅ 接受 | CI/调试（危险） |
//!
//! ## 存储 trait（`KnownHostsStore`）
//!
//! 抽象主机指纹存储，便于在不同上下文注入不同实现：
//! - 应用层：`SqliteStorage`（持久化）
//! - 测试：`InMemoryKnownHosts`（仅内存）
//!
//! ssh-core **不依赖** desktop crate，由 desktop 反向实现此 trait。

use async_trait::async_trait;

use crate::Result;

/// 单条已知主机记录
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct KnownHost {
    /// 目标主机（IP 或域名）
    pub host: String,
    /// 目标端口
    pub port: u16,
    /// 公钥算法名（如 `ssh-ed25519` / `ssh-rsa`）
    pub key_type: String,
    /// 公钥指纹（SHA-256 Base64 形式，由 russh `PublicKey::fingerprint()` 生成）
    pub fingerprint: String,
}

/// 主机密钥校验策略
///
/// 决定 [`crate::connection::SshSession`] 在收到远端公钥后如何处理。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostKeyPolicy {
    /// 严格模式：仅接受已知且指纹匹配的主机
    ///
    /// - 已知且指纹匹配 → 接受
    /// - 已知但指纹不匹配 → 拒绝（疑似 MITM）
    /// - 未知 → 拒绝
    ///
    /// 生产环境推荐。配合 UI 弹窗（阶段 5）让用户主动确认。
    Strict,

    /// 接受新模式：已知严格匹配，未知接受并自动保存
    ///
    /// - 已知且指纹匹配 → 接受
    /// - 已知但指纹不匹配 → 拒绝
    /// - 未知 → 接受并写入 store（下次开始按已知处理）
    ///
    /// 类似 OpenSSH `StrictHostKeyChecking=accept-new`。
    AcceptNew,

    /// 接受所有（开发调试）：无条件接受，不查询不保存
    ///
    /// **危险**：完全跳过 MITM 校验，仅在本地调试/CI 环境使用。
    AcceptAll,
}

impl HostKeyPolicy {
    /// 从 `accept_first_host_key` 标志快速构造策略
    ///
    /// - `true`  → `AcceptNew`（首次接受，后续严格）
    /// - `false` → `Strict`（默认严格）
    ///
    /// 注：保留便捷构造，避免破坏现有 `ConnectionConfig` 字段。
    pub fn from_accept_first(accept_first: bool) -> Self {
        if accept_first {
            HostKeyPolicy::AcceptNew
        } else {
            HostKeyPolicy::Strict
        }
    }
}

/// 已知主机存储 trait
///
/// 应用层（desktop 的 `SqliteStorage`）实现此 trait 并注入 [`crate::connection::SshSession::connect`]。
/// 测试用 [`InMemoryKnownHosts`]。
///
/// 所有方法都是 `async + Send + Sync`，可在 tokio task 间共享。
#[async_trait]
pub trait KnownHostsStore: Send + Sync {
    /// 查找指定 `host:port` 的已知主机记录
    ///
    /// 返回值：
    /// - `Ok(Some(entry))`：已存在记录
    /// - `Ok(None)`：未找到
    /// - `Err(_)`：存储查询失败（按 None 处理，由调用方决定接受/拒绝）
    async fn lookup(&self, host: &str, port: u16) -> Result<Option<KnownHost>>;

    /// 写入一条已知主机记录（UPSERT 语义）
    ///
    /// `AcceptNew` 策略首次接受主机时调用。
    async fn save(&self, entry: KnownHost) -> Result<()>;
}

/// 内存存储（测试 / 临时使用）
///
/// 基于 `tokio::sync::Mutex<HashMap<...>>`，进程结束即丢失。
#[derive(Default)]
pub struct InMemoryKnownHosts {
    inner: tokio::sync::Mutex<std::collections::HashMap<(String, u16), KnownHost>>,
}

impl InMemoryKnownHosts {
    /// 创建空存储
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl KnownHostsStore for InMemoryKnownHosts {
    async fn lookup(&self, host: &str, port: u16) -> Result<Option<KnownHost>> {
        let map = self.inner.lock().await;
        Ok(map.get(&(host.to_string(), port)).cloned())
    }

    async fn save(&self, entry: KnownHost) -> Result<()> {
        let mut map = self.inner.lock().await;
        map.insert((entry.host.clone(), entry.port), entry);
        Ok(())
    }
}

/// 校验给定公钥是否符合策略
///
/// 此函数是**纯函数**：不与 store 交互，仅基于 `known`（已查询结果）判断。
/// 由调用方（如 `HostKeyHandler::check_server_key`）负责 store 查询与写入。
///
/// 返回：
/// - `Ok(true)`：接受
/// - `Ok(false)`：拒绝
///
/// # 参数
/// - `server_key_type`：远端公钥算法名（`PublicKey::name()`）
/// - `server_fingerprint`：远端公钥指纹（`PublicKey::fingerprint().to_string()`）
/// - `known`：store 中已查询的记录（可能为 `None`/`Some`）
/// - `policy`：校验策略
pub fn verify_host_key(
    server_key_type: &str,
    server_fingerprint: &str,
    known: Option<&KnownHost>,
    policy: HostKeyPolicy,
) -> bool {
    match policy {
        HostKeyPolicy::AcceptAll => true,
        HostKeyPolicy::Strict => match known {
            Some(e) => e.fingerprint == server_fingerprint && e.key_type == server_key_type,
            None => false,
        },
        HostKeyPolicy::AcceptNew => match known {
            // 已知：严格匹配
            Some(e) => e.fingerprint == server_fingerprint && e.key_type == server_key_type,
            // 未知：接受（调用方负责 save）
            None => true,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_known(key_type: &str, fp: &str) -> KnownHost {
        KnownHost {
            host: "1.2.3.4".into(),
            port: 22,
            key_type: key_type.into(),
            fingerprint: fp.into(),
        }
    }

    #[test]
    fn test_strict_matches() {
        let k = sample_known("ssh-ed25519", "FP-A");
        assert!(verify_host_key("ssh-ed25519", "FP-A", Some(&k), HostKeyPolicy::Strict));
    }

    #[test]
    fn test_strict_fingerprint_mismatch() {
        let k = sample_known("ssh-ed25519", "FP-A");
        // 指纹不匹配 → 拒绝（疑似 MITM）
        assert!(!verify_host_key("ssh-ed25519", "FP-B", Some(&k), HostKeyPolicy::Strict));
    }

    #[test]
    fn test_strict_key_type_mismatch() {
        let k = sample_known("ssh-ed25519", "FP-A");
        // 算法不匹配 → 拒绝
        assert!(!verify_host_key("ssh-rsa", "FP-A", Some(&k), HostKeyPolicy::Strict));
    }

    #[test]
    fn test_strict_unknown_rejected() {
        assert!(!verify_host_key("ssh-ed25519", "FP-A", None, HostKeyPolicy::Strict));
    }

    #[test]
    fn test_accept_new_unknown_accepted() {
        assert!(verify_host_key("ssh-ed25519", "FP-A", None, HostKeyPolicy::AcceptNew));
    }

    #[test]
    fn test_accept_new_known_mismatch_rejected() {
        let k = sample_known("ssh-ed25519", "FP-A");
        assert!(!verify_host_key("ssh-ed25519", "FP-B", Some(&k), HostKeyPolicy::AcceptNew));
    }

    #[test]
    fn test_accept_all_always_true() {
        assert!(verify_host_key("anything", "X", None, HostKeyPolicy::AcceptAll));
        let k = sample_known("ssh-ed25519", "FP-A");
        assert!(verify_host_key("different", "Y", Some(&k), HostKeyPolicy::AcceptAll));
    }

    #[tokio::test]
    async fn test_in_memory_store_lookup_save() {
        let store = InMemoryKnownHosts::new();
        let k = sample_known("ssh-ed25519", "FP-A");

        // 初始无记录
        assert!(store.lookup("1.2.3.4", 22).await.unwrap().is_none());

        // 保存后可查到
        store.save(k.clone()).await.unwrap();
        let got = store.lookup("1.2.3.4", 22).await.unwrap().unwrap();
        assert_eq!(got.fingerprint, "FP-A");

        // 不同端口查不到
        assert!(store.lookup("1.2.3.4", 2222).await.unwrap().is_none());
    }
}
