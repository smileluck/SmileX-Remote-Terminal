//! # 密钥管理
//!
//! MVP 阶段仅提供基础结构（密钥类型枚举、生成/解析入口）。
//! 生成/解析的具体实现详见架构 §2.5，后续阶段补全。

use crate::error::Result;

/// 密钥算法类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum KeyType {
    /// Ed25519（推荐）
    Ed25519,
    /// RSA（兼容性）
    Rsa,
    /// ECDSA
    Ecdsa,
}

impl KeyType {
    /// 转字符串（OpenSSH 算法名）
    pub fn as_str(&self) -> &'static str {
        match self {
            KeyType::Ed25519 => "ssh-ed25519",
            KeyType::Rsa => "ssh-rsa",
            KeyType::Ecdsa => "ecdsa-sha2-nistp256",
        }
    }
}

/// 密钥生成配置
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct KeyGenConfig {
    /// 算法
    pub key_type: KeyType,
    /// 注释
    pub comment: Option<String>,
    /// passphrase（私钥保护）
    pub passphrase: Option<String>,
}

impl Default for KeyGenConfig {
    fn default() -> Self {
        Self {
            key_type: KeyType::Ed25519,
            comment: None,
            passphrase: None,
        }
    }
}

/// 密钥生成结果
#[derive(Debug, Clone)]
pub struct GeneratedKey {
    /// 私钥 PEM（OpenSSH 格式）
    pub private_key_pem: String,
    /// 公钥（OpenSSH 格式，一行字符串）
    pub public_key: String,
    /// 指纹 SHA256
    pub fingerprint: String,
}

/// 生成密钥对
///
/// MVP 占位：后续阶段补全 russh-keys 的具体调用。
pub async fn generate(_config: &KeyGenConfig) -> Result<GeneratedKey> {
    Err(crate::error::Error::Key(
        "密钥生成尚未实现（阶段 5）".into(),
    ))
}

/// 解析 OpenSSH 格式私钥（返回指纹用于校验）
///
/// MVP 占位。
pub fn parse_private_key(_pem: &str, _passphrase: Option<&str>) -> Result<String> {
    Err(crate::error::Error::Key(
        "密钥解析尚未实现（阶段 5）".into(),
    ))
}
