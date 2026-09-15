//! # AI 命令执行安全闸门命令组
//!
//! AI 发起的远程命令执行统一走后端闸门（此前安全分类只在前端，可被绕过）：
//! - 分类：`ai_core::safety::classify_command`（与前端 `classifyCommand` 语义一致）
//! - 闸门规则：ReadOnly 恒放行；Modify 命中白名单放行，否则需用户确认；
//!   Danger 永不因白名单放行，必须显式 `approved = true`
//! - 绑定校验：chat → profile → 活跃 SSH 会话（session_meta 身份匹配），
//!   未绑定/断连一律拒绝，绝不静默打到别的机器
//! - 审计：拒绝与执行完成全量落 `ai_audit_log`；`decision` 表示授权决策
//!   （auto/approved/rejected/failed），非远端执行成败（非零退出码不算 failed）
//!
//! 两条执行路径：
//! - PTY 可见路径：前端 `ai_exec_prepare` 预检 → 写 `session_input` 执行
//!   → 完成后 `ai_exec_finish` 补记审计
//! - 非交互通道：`ai_exec_command` 一站式（闸门 + `SshSession::exec_with_status` + 审计）

use serde::Serialize;
use tauri::State;

use ai_core::{classify_command, CommandRisk};

use crate::error::AppError;
use crate::storage::sqlite::{AiAllowlistEntry, AiAuditEntry};
use crate::AppState;

/// AI 命令执行超时（非交互通道；比监控采集的 10s 宽，AI 命令可能耗时较长）
const AI_EXEC_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);
/// 返回前端的输出上限（8KB，超出保留尾部——报错通常在末尾）
const OUTPUT_CAP: usize = 8 * 1024;
/// 审计分页默认/上限
const AUDIT_DEFAULT_LIMIT: i64 = 50;
const AUDIT_MAX_LIMIT: i64 = 200;

/// `ai_exec_prepare` 预检结果
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecPrepareResult {
    pub risk: CommandRisk,
    /// 闸门决策：`allowed`（可执行）/ `needs_approval`（Modify 未命中白名单且
    /// 未批准，需用户确认后带 approved=true 重试，不记审计）/ `rejected`
    /// （硬拒绝：绑定失效或 Danger 未批准，已记 rejected 审计）
    pub status: String,
    /// 拒绝/待确认原因：`chat_not_found` / `chat_not_bound` / `session_disconnected` /
    /// `danger_requires_confirmation` / `not_in_allowlist`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// 硬拒绝时写入的审计记录 id
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audit_id: Option<i64>,
}

/// `ai_exec_command` 一站式执行结果
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecResult {
    pub risk: CommandRisk,
    /// `executed` / `needs_approval` / `rejected` / `failed`
    pub status: String,
    /// 命令输出（截断到 8KB 保尾；仅 executed 有值）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    /// 失败/拒绝原因
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// 审计记录 id（rejected/executed/failed 有值；needs_approval 不记审计）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audit_id: Option<i64>,
}

/// 闸门解析出的执行目标
struct ResolvedTarget {
    session_id: String,
    profile_id: String,
}

/// 风险等级 → 落库字符串（与 serde snake_case 一致）
fn risk_str(risk: CommandRisk) -> &'static str {
    match risk {
        CommandRisk::ReadOnly => "read_only",
        CommandRisk::Modify => "modify",
        CommandRisk::Danger => "danger",
    }
}

/// 校验来源标记（manual / auto / plan）
fn validate_source(source: &str) -> Result<(), AppError> {
    match source {
        "manual" | "auto" | "plan" => Ok(()),
        _ => Err(AppError::ai(format!(
            "非法的命令来源 source={source:?}（应为 manual/auto/plan）"
        ))),
    }
}

/// 白名单命中判定：scope 已由 SQL 过滤，这里比对 risk 与 pattern
///
/// pattern 匹配规则：与完整命令（trim 后）相等，或为命令前缀且后随空白。
/// 条目 risk 必须与命令当前分类一致，防止低等级条目放大权限。
fn whitelist_hit(entries: &[AiAllowlistEntry], command: &str, risk: CommandRisk) -> bool {
    let cmd = command.trim();
    let risk = risk_str(risk);
    entries.iter().any(|e| {
        if e.risk != risk {
            return false;
        }
        let p = e.pattern.trim();
        cmd == p || (cmd.len() > p.len() && cmd.starts_with(p) && cmd[p.len()..].starts_with(char::is_whitespace))
    })
}

/// 闸门决策（纯函数，便于单测）：返回 (status, reason)
///
/// - ReadOnly → 恒 `allowed`
/// - Modify → 命中白名单或 `approved=true` 放行；否则 `needs_approval`（软拒绝，不记审计）
/// - Danger → 仅 `approved=true` 放行；否则 `rejected`（硬拒绝，记审计）
fn gate_decision(risk: CommandRisk, whitelisted: bool, approved: bool) -> (&'static str, Option<&'static str>) {
    match risk {
        CommandRisk::ReadOnly => ("allowed", None),
        CommandRisk::Modify => {
            if whitelisted || approved {
                ("allowed", None)
            } else {
                ("needs_approval", Some("not_in_allowlist"))
            }
        }
        CommandRisk::Danger => {
            if approved {
                ("allowed", None)
            } else {
                ("rejected", Some("danger_requires_confirmation"))
            }
        }
    }
}

/// 解析 chat 绑定的目标 SSH 会话（闸门第一步）
///
/// chat.profile_id → 档案的 `user@host:port` 身份 → session_meta 中找活跃会话；
/// profile_id 不是档案 id（旧数据/快速连接回退的 tab 标题）时直接与
/// session_meta 身份值比对兜底。
async fn resolve_chat_session(state: &AppState, chat_id: &str) -> Result<ResolvedTarget, String> {
    let chat = state
        .storage
        .get_chat(chat_id)
        .await
        .map_err(|e| format!("查询会话绑定失败: {e}"))?
        .ok_or_else(|| "chat_not_found".to_string())?;

    if chat.profile_id.is_empty() {
        return Err("chat_not_bound".to_string());
    }

    // 档案存在 → 按身份（user@host:port）匹配活跃会话；
    // 否则按旧数据的区分键直接与身份值比对
    let identity = match state.storage.get_profile(&chat.profile_id).await {
        Ok(Some(p)) => format!("{}@{}:{}", p.username, p.host, p.port),
        Ok(None) => chat.profile_id.clone(),
        Err(e) => return Err(format!("查询会话配置失败: {e}")),
    };

    let metas = state.session_meta.lock().await;
    for (session_id, meta) in metas.iter() {
        if *meta == identity && state.ssh_manager.get(session_id).await.is_some() {
            return Ok(ResolvedTarget {
                session_id: session_id.clone(),
                profile_id: chat.profile_id.clone(),
            });
        }
    }
    Err("session_disconnected".to_string())
}

/// 写审计（失败只记日志不阻断闸门主流程），返回记录 id
#[allow(clippy::too_many_arguments)]
async fn write_audit(
    state: &AppState,
    chat_id: Option<&str>,
    profile_id: Option<&str>,
    command: &str,
    risk: CommandRisk,
    decision: &str,
    source: &str,
    exit_code: Option<i64>,
    duration_ms: Option<i64>,
) -> Option<i64> {
    let ts = chrono::Utc::now().timestamp();
    match state
        .storage
        .ai_audit_insert(ts, chat_id, profile_id, command, risk_str(risk), decision, source, exit_code, duration_ms)
        .await
    {
        Ok(id) => Some(id),
        Err(e) => {
            tracing::warn!(error = %e, command, decision, "AI 命令审计写入失败");
            None
        }
    }
}

/// 截断到 8KB 保尾（char 边界安全；报错信息通常在输出末尾）
fn truncate_tail(s: &str) -> String {
    if s.len() <= OUTPUT_CAP {
        return s.to_string();
    }
    let cut = s.len() - OUTPUT_CAP;
    let start = s
        .char_indices()
        .map(|(i, _)| i)
        .find(|&i| i >= cut)
        .unwrap_or(s.len());
    s[start..].to_string()
}

/// 纯分类（供前端即时 UI 提示，不做任何闸门/审计动作）
#[tauri::command]
pub async fn ai_classify_command(command: String) -> Result<CommandRisk, AppError> {
    Ok(classify_command(&command))
}

/// 执行预检（PTY 可见路径用）：分类 + 绑定校验 + 白名单判定 + 批准状态
///
/// - `allowed`：可执行，不写审计（执行完成后由 `ai_exec_finish` 补记）
/// - `needs_approval`：Modify 未命中白名单且未批准——软拒绝，**不记审计**，
///   前端弹窗确认后带 `approved=true` 重新预检
/// - `rejected`：硬拒绝（绑定失效 / Danger 未批准），立即写 `rejected` 审计
#[tauri::command]
pub async fn ai_exec_prepare(
    state: State<'_, AppState>,
    chat_id: String,
    command: String,
    source: String,
    approved: bool,
) -> Result<ExecPrepareResult, AppError> {
    validate_source(&source)?;
    let risk = classify_command(&command);

    // 绑定校验：chat → profile → 活跃会话
    let profile_id = state
        .storage
        .get_chat(&chat_id)
        .await
        .ok()
        .flatten()
        .map(|c| c.profile_id)
        .filter(|p| !p.is_empty());
    let target = match resolve_chat_session(&state, &chat_id).await {
        Ok(t) => t,
        Err(reason) => {
            let audit_id = write_audit(
                &state, Some(&chat_id), profile_id.as_deref(),
                &command, risk, "rejected", &source, None, None,
            ).await;
            return Ok(ExecPrepareResult {
                risk,
                status: "rejected".into(),
                reason: Some(reason),
                audit_id,
            });
        }
    };

    // 白名单判定（Danger 永不因白名单放行，无需查询）
    let whitelisted = match risk {
        CommandRisk::ReadOnly => true,
        CommandRisk::Modify => {
            let entries = state
                .storage
                .ai_allowlist_for_context(&chat_id, &target.profile_id)
                .await
                .map_err(|e| AppError::storage(format!("查询 AI 命令白名单失败: {e}")))?;
            whitelist_hit(&entries, &command, risk)
        }
        CommandRisk::Danger => false,
    };

    let (status, reason) = gate_decision(risk, whitelisted, approved);
    match status {
        "allowed" => Ok(ExecPrepareResult {
            risk,
            status: status.into(),
            reason: None,
            audit_id: None,
        }),
        // needs_approval：软拒绝，等用户确认，不记审计
        "needs_approval" => Ok(ExecPrepareResult {
            risk,
            status: status.into(),
            reason: reason.map(str::to_string),
            audit_id: None,
        }),
        // rejected：硬拒绝（Danger 未批准），记审计
        _ => {
            let audit_id = write_audit(
                &state, Some(&chat_id), Some(&target.profile_id),
                &command, risk, "rejected", &source, None, None,
            ).await;
            Ok(ExecPrepareResult {
                risk,
                status: status.into(),
                reason: reason.map(str::to_string),
                audit_id,
            })
        }
    }
}

/// PTY 可见路径执行完成后补记审计
///
/// decision 由 risk 推出：read_only = `auto`（只读自动放行），
/// modify/danger = `approved`（能走到执行说明已经过了确认/白名单）。
/// 非零 exit_code 不影响 decision（决策 ≠ 执行成败）。
#[tauri::command]
pub async fn ai_exec_finish(
    state: State<'_, AppState>,
    chat_id: String,
    command: String,
    risk: CommandRisk,
    source: String,
    exit_code: Option<i64>,
    duration_ms: Option<i64>,
) -> Result<(), AppError> {
    validate_source(&source)?;
    let profile_id = state
        .storage
        .get_chat(&chat_id)
        .await
        .ok()
        .flatten()
        .map(|c| c.profile_id)
        .filter(|p| !p.is_empty());
    let decision = match risk {
        CommandRisk::ReadOnly => "auto",
        _ => "approved",
    };
    write_audit(
        &state, Some(&chat_id), profile_id.as_deref(),
        &command, risk, decision, &source, exit_code, duration_ms,
    ).await;
    Ok(())
}

/// 非交互通道一站式执行：闸门 + exec + 审计
///
/// - Danger 且 `approved=false` → 拒绝（记 rejected 审计）
/// - Modify 未命中白名单且 `approved=false` → `needs_approval`（不执行、不记审计）
/// - `approved=true` 且 `remember` 为 chat/profile/global 且非 Danger → 写入白名单
/// - 执行完成记 auto/approved 审计；闸门内部执行错误（超时/通道失败）记 failed
#[tauri::command]
pub async fn ai_exec_command(
    state: State<'_, AppState>,
    chat_id: String,
    command: String,
    source: String,
    approved: bool,
    remember: Option<String>,
) -> Result<ExecResult, AppError> {
    validate_source(&source)?;
    let risk = classify_command(&command);

    // 1) 绑定校验
    let known_profile_id = state
        .storage
        .get_chat(&chat_id)
        .await
        .ok()
        .flatten()
        .map(|c| c.profile_id)
        .filter(|p| !p.is_empty());
    let target = match resolve_chat_session(&state, &chat_id).await {
        Ok(t) => t,
        Err(reason) => {
            let audit_id = write_audit(
                &state, Some(&chat_id), known_profile_id.as_deref(),
                &command, risk, "rejected", &source, None, None,
            ).await;
            return Ok(ExecResult {
                risk,
                status: "rejected".into(),
                output: None,
                error: Some(reason),
                audit_id,
            });
        }
    };

    // 2) 白名单判定
    let entries = state
        .storage
        .ai_allowlist_for_context(&chat_id, &target.profile_id)
        .await
        .map_err(|e| AppError::storage(format!("查询 AI 命令白名单失败: {e}")))?;
    let whitelisted = risk != CommandRisk::Danger && whitelist_hit(&entries, &command, risk);

    // 3) 闸门
    if risk == CommandRisk::Danger && !approved {
        let audit_id = write_audit(
            &state, Some(&chat_id), Some(&target.profile_id),
            &command, risk, "rejected", &source, None, None,
        ).await;
        return Ok(ExecResult {
            risk,
            status: "rejected".into(),
            output: None,
            error: Some("danger_requires_confirmation".into()),
            audit_id,
        });
    }
    if risk == CommandRisk::Modify && !whitelisted && !approved {
        return Ok(ExecResult {
            risk,
            status: "needs_approval".into(),
            output: None,
            error: None,
            audit_id: None,
        });
    }

    // 4) 记住选择：approved + remember + 非 Danger → 写白名单
    if approved && risk != CommandRisk::Danger {
        if let Some(scope) = remember.as_deref() {
            let scope_id = match scope {
                "chat" => Some(chat_id.as_str()),
                "profile" => Some(target.profile_id.as_str()),
                "global" => None,
                _ => {
                    return Err(AppError::ai(format!(
                        "非法的白名单范围 remember={scope:?}（应为 chat/profile/global）"
                    )))
                }
            };
            if let Err(e) = state
                .storage
                .ai_allowlist_add(command.trim(), risk_str(risk), scope, scope_id, chrono::Utc::now().timestamp())
                .await
            {
                // 白名单写入失败不阻断本次执行（本次已获批准）
                tracing::warn!(error = %e, command, "AI 命令白名单写入失败");
            }
        }
    }

    // 5) 执行（非交互 exec 通道，独立超时）
    let session = state
        .ssh_manager
        .get(&target.session_id)
        .await
        .ok_or_else(|| AppError::session(format!("会话 {} 不存在或已断开", target.session_id)))?;
    let started = std::time::Instant::now();
    let outcome = tokio::time::timeout(AI_EXEC_TIMEOUT, session.exec_with_status(&command)).await;
    let duration_ms = started.elapsed().as_millis() as i64;
    let decision = if approved { "approved" } else { "auto" };

    match outcome {
        Ok(Ok(exec_out)) => {
            let exit_code = exec_out.exit_code.map(|c| c as i64);
            let audit_id = write_audit(
                &state, Some(&chat_id), Some(&target.profile_id),
                &command, risk, decision, &source, exit_code, Some(duration_ms),
            ).await;
            Ok(ExecResult {
                risk,
                status: "executed".into(),
                output: Some(truncate_tail(&exec_out.output)),
                error: None,
                audit_id,
            })
        }
        Ok(Err(e)) => {
            let msg = format!("执行失败: {e}");
            let audit_id = write_audit(
                &state, Some(&chat_id), Some(&target.profile_id),
                &command, risk, "failed", &source, None, Some(duration_ms),
            ).await;
            Ok(ExecResult {
                risk,
                status: "failed".into(),
                output: None,
                error: Some(msg),
                audit_id,
            })
        }
        Err(_) => {
            let msg = format!("执行超时（>{}s）", AI_EXEC_TIMEOUT.as_secs());
            let audit_id = write_audit(
                &state, Some(&chat_id), Some(&target.profile_id),
                &command, risk, "failed", &source, None, Some(duration_ms),
            ).await;
            Ok(ExecResult {
                risk,
                status: "failed".into(),
                output: None,
                error: Some(msg),
                audit_id,
            })
        }
    }
}

/// 新增白名单条目（Danger 级 pattern 拒绝入表）
#[tauri::command]
pub async fn ai_allowlist_add(
    state: State<'_, AppState>,
    pattern: String,
    risk: CommandRisk,
    scope: String,
    scope_id: Option<String>,
) -> Result<AiAllowlistEntry, AppError> {
    let pattern = pattern.trim().to_string();
    if pattern.is_empty() {
        return Err(AppError::ai("白名单 pattern 不能为空"));
    }
    if risk == CommandRisk::Danger || classify_command(&pattern) == CommandRisk::Danger {
        return Err(AppError::ai("危险命令不允许加入白名单"));
    }
    let scope_id = match scope.as_str() {
        "chat" | "profile" => Some(
            scope_id
                .filter(|s| !s.is_empty())
                .ok_or_else(|| AppError::ai(format!("scope={scope} 时 scope_id 不能为空")))?,
        ),
        "global" => None,
        _ => return Err(AppError::ai(format!("非法的白名单范围 scope={scope:?}"))),
    };
    state
        .storage
        .ai_allowlist_add(&pattern, risk_str(risk), &scope, scope_id.as_deref(), chrono::Utc::now().timestamp())
        .await
        .map_err(|e| AppError::storage(format!("写入 AI 命令白名单失败: {e}")))
}

/// 删除白名单条目（返回是否存在）
#[tauri::command]
pub async fn ai_allowlist_remove(state: State<'_, AppState>, id: i64) -> Result<bool, AppError> {
    state
        .storage
        .ai_allowlist_remove(id)
        .await
        .map_err(|e| AppError::storage(format!("删除 AI 命令白名单失败: {e}")))
}

/// 某执行上下文（chat + profile）可见的白名单条目（global + 该 profile + 该 chat）
#[tauri::command]
pub async fn ai_allowlist_list(
    state: State<'_, AppState>,
    chat_id: String,
    profile_id: String,
) -> Result<Vec<AiAllowlistEntry>, AppError> {
    state
        .storage
        .ai_allowlist_for_context(&chat_id, &profile_id)
        .await
        .map_err(|e| AppError::storage(format!("查询 AI 命令白名单失败: {e}")))
}

/// 全部白名单条目（设置页只读视图用，不限执行上下文）
#[tauri::command]
pub async fn ai_allowlist_all(
    state: State<'_, AppState>,
) -> Result<Vec<AiAllowlistEntry>, AppError> {
    state
        .storage
        .ai_allowlist_list_all()
        .await
        .map_err(|e| AppError::storage(format!("查询 AI 命令白名单失败: {e}")))
}

/// 分页查询审计日志（ts 倒序；chat_id 为 None 查全部）
#[tauri::command]
pub async fn ai_audit_list(
    state: State<'_, AppState>,
    chat_id: Option<String>,
    limit: Option<u32>,
    offset: Option<u32>,
) -> Result<Vec<AiAuditEntry>, AppError> {
    let limit = limit
        .map(|l| (l as i64).clamp(1, AUDIT_MAX_LIMIT))
        .unwrap_or(AUDIT_DEFAULT_LIMIT);
    let offset = offset.unwrap_or(0) as i64;
    state
        .storage
        .ai_audit_list(chat_id.as_deref(), limit, offset)
        .await
        .map_err(|e| AppError::storage(format!("查询 AI 命令审计失败: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(pattern: &str, risk: &str) -> AiAllowlistEntry {
        AiAllowlistEntry {
            id: 0,
            pattern: pattern.into(),
            risk: risk.into(),
            scope: "global".into(),
            scope_id: None,
            created_at: 0,
        }
    }

    #[test]
    fn test_whitelist_hit() {
        let entries = vec![entry("docker ps", "modify"), entry("df -h", "read_only")];
        // 精确匹配与前缀匹配
        assert!(whitelist_hit(&entries, "docker ps", CommandRisk::Modify));
        assert!(whitelist_hit(&entries, "docker ps -a", CommandRisk::Modify));
        // 前缀后必须随空白（docker psx 不命中）
        assert!(!whitelist_hit(&entries, "docker psx", CommandRisk::Modify));
        // risk 不一致不命中（防止低等级条目放大权限）
        assert!(!whitelist_hit(&entries, "docker ps", CommandRisk::ReadOnly));
        assert!(!whitelist_hit(&entries, "rm -rf /", CommandRisk::Danger));
        // 前后空白 trim 后匹配
        assert!(whitelist_hit(&entries, "  df -h  ", CommandRisk::ReadOnly));
    }

    #[test]
    fn test_truncate_tail() {
        let short = "abc";
        assert_eq!(truncate_tail(short), "abc");
        let long = "x".repeat(OUTPUT_CAP + 100);
        let out = truncate_tail(&long);
        assert_eq!(out.len(), OUTPUT_CAP);
        // 多字节字符不切坏
        let mb = format!("{}", "汉".repeat(OUTPUT_CAP));
        let out = truncate_tail(&mb);
        assert!(out.len() <= OUTPUT_CAP + 3);
        assert!(out.ends_with('汉'));
    }

    #[test]
    fn test_gate_decision() {
        // ReadOnly 恒放行
        assert_eq!(gate_decision(CommandRisk::ReadOnly, false, false), ("allowed", None));
        // Modify：白名单或批准放行，否则 needs_approval（软拒绝，不记审计）
        assert_eq!(gate_decision(CommandRisk::Modify, true, false), ("allowed", None));
        assert_eq!(gate_decision(CommandRisk::Modify, false, true), ("allowed", None));
        assert_eq!(
            gate_decision(CommandRisk::Modify, false, false),
            ("needs_approval", Some("not_in_allowlist"))
        );
        // Danger：白名单无效，仅显式批准放行；未批准为硬拒绝（记审计）
        assert_eq!(gate_decision(CommandRisk::Danger, true, false).0, "rejected");
        assert_eq!(
            gate_decision(CommandRisk::Danger, false, false),
            ("rejected", Some("danger_requires_confirmation"))
        );
        assert_eq!(gate_decision(CommandRisk::Danger, false, true), ("allowed", None));
    }
}
