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
/// - Ed25519（推荐，快速且安全）
/// - RSA（4096 位，兼容旧服务器）
/// 私钥为 PKCS#8 PEM（未加密，调用方自行决定是否加 passphrase 层）。
pub async fn generate(config: &KeyGenConfig) -> Result<GeneratedKey> {
    let key_pair = match config.key_type {
        KeyType::Ed25519 => russh_keys::key::KeyPair::generate_ed25519()
            .ok_or_else(|| crate::error::Error::Key("生成 Ed25519 密钥失败".into()))?,
        KeyType::Rsa => russh_keys::key::KeyPair::generate_rsa(
            4096,
            russh_keys::key::SignatureHash::SHA2_256,
        )
        .ok_or_else(|| crate::error::Error::Key("生成 RSA 密钥失败".into()))?,
        KeyType::Ecdsa => {
            return Err(crate::error::Error::Key(
                "暂不支持 ECDSA 密钥生成（推荐 Ed25519）".into(),
            ))
        }
    };

    // 私钥 → PKCS#8 PEM
    let mut pem_buf = Vec::new();
    russh_keys::encode_pkcs8_pem(&key_pair, &mut pem_buf)
        .map_err(|e| crate::error::Error::Key(format!("编码私钥 PEM 失败: {e}")))?;
    let private_key_pem = String::from_utf8_lossy(&pem_buf).into_owned();

    // 公钥 → OpenSSH 一行格式
    let public_key = public_key_line(&key_pair)?;

    // 指纹
    let fingerprint = key_pair
        .clone_public_key()
        .map_err(|e| crate::error::Error::Key(format!("提取公钥失败: {e}")))?
        .fingerprint();

    Ok(GeneratedKey {
        private_key_pem,
        public_key,
        fingerprint,
    })
}

/// 由 KeyPair 提取 OpenSSH 格式公钥行（`algo base64`）
fn public_key_line(key_pair: &russh_keys::key::KeyPair) -> Result<String> {
    let public = key_pair
        .clone_public_key()
        .map_err(|e| crate::error::Error::Key(format!("提取公钥失败: {e}")))?;
    let name = public.name().to_string();
    let mut b64 = Vec::new();
    russh_keys::write_public_key_base64(&mut b64, &public)
        .map_err(|e| crate::error::Error::Key(format!("编码公钥失败: {e}")))?;
    let b64_str = String::from_utf8_lossy(&b64).trim().to_string();
    Ok(format!("{name} {b64_str}"))
}

/// 解析 OpenSSH 格式私钥（返回指纹 + 公钥行，用于导入校验）
pub fn parse_private_key(
    pem: &str,
    passphrase: Option<&str>,
) -> Result<(String, String)> {
    let key_pair = russh_keys::decode_secret_key(pem, passphrase)
        .map_err(|e| crate::error::Error::Key(format!("解析私钥失败: {e}")))?;
    let public = key_pair
        .clone_public_key()
        .map_err(|e| crate::error::Error::Key(format!("提取公钥失败: {e}")))?;
    let fingerprint = public.fingerprint();
    let public_key = public_key_line(&key_pair)?;
    Ok((fingerprint, public_key))
}
