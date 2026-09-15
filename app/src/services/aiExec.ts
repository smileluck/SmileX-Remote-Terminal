/**
 * AI 命令执行安全闸门命令组封装
 *
 * 前端所有 AI 发起命令的执行决策统一走后端闸门：
 * - `classifyCommand`：纯分类（即时 UI 风险标签，无闸门/审计副作用）
 * - `execPrepare`：PTY 可见路径预检（allowed / needs_approval / rejected）
 * - `execFinish`：PTY 路径执行完成后补记审计
 * - `execCommand`：非交互通道一站式执行（自带闸门 + 审计 + 8KB 保尾截断）
 * - 白名单 / 审计日志查询
 */

import { invoke } from './invoke'

/** 后端命令风险分级（ai_core::safety::CommandRisk，snake_case） */
export type CommandRisk = 'read_only' | 'modify' | 'danger'

/** 命令来源标记 */
export type ExecSource = 'manual' | 'auto' | 'plan'

/** `ai_exec_prepare` 预检结果 */
export interface ExecPrepareResult {
  risk: CommandRisk
  /** allowed = 可执行；needs_approval = 需用户确认（未记审计）；rejected = 硬拒绝（已记审计） */
  status: 'allowed' | 'needs_approval' | 'rejected'
  reason?: string
  auditId?: number
}

/** `ai_exec_command` 一站式执行结果 */
export interface ExecResult {
  risk: CommandRisk
  status: 'executed' | 'needs_approval' | 'rejected' | 'failed'
  output?: string
  error?: string
  auditId?: number
}

/** 白名单条目 */
export interface AiAllowlistEntry {
  id: number
  pattern: string
  risk: 'read_only' | 'modify'
  scope: 'chat' | 'profile' | 'global'
  scopeId?: string
  createdAt: number
}

/** 审计日志条目 */
export interface AiAuditEntry {
  id: number
  ts: number
  chatId?: string
  profileId?: string
  command: string
  risk: CommandRisk
  decision: 'auto' | 'approved' | 'rejected' | 'failed'
  source: ExecSource
  exitCode?: number
  durationMs?: number
}

/** 纯分类（即时 UI 提示用，不做闸门/审计） */
export function classifyCommand(command: string): Promise<CommandRisk> {
  return invoke<CommandRisk>('ai_classify_command', { command })
}

/** 执行预检（PTY 可见路径；needs_approval 时不记审计，确认后带 approved=true 重试） */
export function execPrepare(
  chatId: string,
  command: string,
  source: ExecSource,
  approved: boolean,
): Promise<ExecPrepareResult> {
  return invoke<ExecPrepareResult>('ai_exec_prepare', { chatId, command, source, approved })
}

/** PTY 路径执行完成后补记审计 */
export function execFinish(
  chatId: string,
  command: string,
  risk: CommandRisk,
  source: ExecSource,
  exitCode?: number,
  durationMs?: number,
): Promise<void> {
  return invoke<void>('ai_exec_finish', { chatId, command, risk, source, exitCode, durationMs })
}

/** 非交互通道一站式执行（自带闸门 + 审计 + 输出截断） */
export function execCommand(
  chatId: string,
  command: string,
  source: ExecSource,
  approved: boolean,
  remember?: 'chat' | 'profile' | 'global',
): Promise<ExecResult> {
  return invoke<ExecResult>('ai_exec_command', { chatId, command, source, approved, remember })
}

/** 新增白名单条目（danger 级会被后端拒绝） */
export function allowlistAdd(
  pattern: string,
  risk: CommandRisk,
  scope: 'chat' | 'profile' | 'global',
  scopeId?: string,
): Promise<AiAllowlistEntry> {
  return invoke<AiAllowlistEntry>('ai_allowlist_add', { pattern, risk, scope, scopeId })
}

/** 删除白名单条目 */
export function allowlistRemove(id: number): Promise<boolean> {
  return invoke<boolean>('ai_allowlist_remove', { id })
}

/** 全部白名单条目（设置页只读视图） */
export function allowlistAll(): Promise<AiAllowlistEntry[]> {
  return invoke<AiAllowlistEntry[]>('ai_allowlist_all')
}

/** 审计日志（ts 倒序；chatId 为空查全部） */
export function auditList(chatId?: string, limit?: number, offset?: number): Promise<AiAuditEntry[]> {
  return invoke<AiAuditEntry[]>('ai_audit_list', { chatId, limit, offset })
}
