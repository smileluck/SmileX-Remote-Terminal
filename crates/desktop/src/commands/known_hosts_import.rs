//! # 本机 known_hosts 扫描命令
//!
//! 解析 `~/.ssh/known_hosts` 中的主机条目，供前端导入为 SSH 会话
//! （归入「本机known-host」分组）。
//!
//! ## OpenSSH known_hosts 行格式
//! ```text
//! [@marker] host[,host2,...] key-type base64-key [comment]
//! ```
//! - `@cert-authority` / `@revoked` marker 行跳过；
//! - `|1|salt|hash` 哈希主机名（HashKnownHosts）无法还原，跳过；
//! - `[host]:port` 方括号格式解析出非标准端口（默认 22）。

use serde::Serialize;

use crate::error::AppError;

/// 扫描到的主机条目
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ScannedHost {
    pub host: String,
    pub port: u16,
}

/// 扫描本机 `~/.ssh/known_hosts`，返回可导入的主机列表（按 host+port 去重）
///
/// 文件不存在时返回空列表（视为无可导入项，不报错）。
#[tauri::command]
pub async fn known_hosts_scan() -> Result<Vec<ScannedHost>, AppError> {
    let path = dirs::home_dir()
        .ok_or_else(|| AppError::storage("无法定位用户主目录"))?
        .join(".ssh")
        .join("known_hosts");

    if !path.exists() {
        return Ok(Vec::new());
    }

    let content = tokio::fs::read_to_string(&path)
        .await
        .map_err(|e| AppError::storage(format!("读取 known_hosts 失败: {e}")))?;

    Ok(parse_known_hosts(&content))
}

/// 解析 known_hosts 内容，提取主机与端口（保序去重）
fn parse_known_hosts(content: &str) -> Vec<ScannedHost> {
    let mut seen = std::collections::HashSet::new();
    let mut hosts = Vec::new();

    for line in content.lines() {
        let line = line.trim();
        // @cert-authority / @revoked 等 marker 行整行跳过
        if line.is_empty() || line.starts_with('#') || line.starts_with('@') {
            continue;
        }

        let hosts_field = match line.split(char::is_whitespace).next() {
            Some(f) if !f.is_empty() => f,
            _ => continue,
        };

        for entry in hosts_field.split(',') {
            // 哈希主机名（|1|salt|hash）无法还原，跳过；
            // 通配符模式（* ?）与取反（!）不是可连接的具体主机，同样跳过
            if entry.is_empty()
                || entry.starts_with('|')
                || entry.starts_with('!')
                || entry.contains('*')
                || entry.contains('?')
            {
                continue;
            }
            if let Some((host, port)) = parse_host_entry(entry) {
                if seen.insert((host.clone(), port)) {
                    hosts.push(ScannedHost { host, port });
                }
            }
        }
    }

    hosts
}

/// 解析单个主机条目：`host`（端口 22）或 `[host]:port`
fn parse_host_entry(entry: &str) -> Option<(String, u16)> {
    if let Some(rest) = entry.strip_prefix('[') {
        let (host, port_str) = rest.split_once("]:")?;
        let port: u16 = port_str.parse().ok()?;
        if host.is_empty() {
            return None;
        }
        Some((host.to_string(), port))
    } else if !entry.is_empty() {
        Some((entry.to_string(), 22))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_host_default_port() {
        let out = parse_known_hosts("example.com ssh-ed25519 AAAAC3NzaC comment");
        assert_eq!(
            out,
            vec![ScannedHost {
                host: "example.com".into(),
                port: 22
            }]
        );
    }

    #[test]
    fn parses_bracketed_port() {
        let out = parse_known_hosts("[10.0.0.1]:2222 ssh-rsa AAAAB3Nz");
        assert_eq!(
            out,
            vec![ScannedHost {
                host: "10.0.0.1".into(),
                port: 2222
            }]
        );
    }

    #[test]
    fn parses_comma_separated_hosts() {
        let out = parse_known_hosts("alias,192.168.1.10 ssh-ed25519 AAAAC3Nz");
        assert_eq!(
            out,
            vec![
                ScannedHost {
                    host: "alias".into(),
                    port: 22
                },
                ScannedHost {
                    host: "192.168.1.10".into(),
                    port: 22
                },
            ]
        );
    }

    #[test]
    fn skips_comments_blanks_markers_and_hashed() {
        let content = "\
# comment line

@cert-authority *.example.com ssh-ed25519 AAAAC3Nz
@revoked bad.host ssh-ed25519 AAAAC3Nz
|1|somesalt|somehash ssh-ed25519 AAAAC3Nz
good.host ssh-ed25519 AAAAC3Nz
";
        let out = parse_known_hosts(content);
        assert_eq!(
            out,
            vec![ScannedHost {
                host: "good.host".into(),
                port: 22
            }]
        );
    }

    #[test]
    fn dedupes_same_host_across_key_types() {
        let content = "\
example.com ssh-ed25519 AAAAC3Nz
example.com ssh-rsa AAAAB3Nz
[example.com]:2222 ssh-ed25519 AAAAC3Nz
";
        let out = parse_known_hosts(content);
        assert_eq!(
            out,
            vec![
                ScannedHost {
                    host: "example.com".into(),
                    port: 22
                },
                ScannedHost {
                    host: "example.com".into(),
                    port: 2222
                },
            ]
        );
    }

    #[test]
    fn ignores_garbage_lines() {
        let out = parse_known_hosts("\n   \n[]:abc ssh-rsa x\n[]:22 ssh-rsa x\n");
        assert_eq!(out, vec![]);
    }
}
