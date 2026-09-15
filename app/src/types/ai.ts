/**
 * AI 模块类型定义
 *
 * LLM Provider 相关类型已迁移至 `@/types/settings`（统一设置/Chat 模块）。
 * 此文件保留向后兼容的重新导出，并定义 Chat 专用类型。
 */

// 重新导出 LLM 类型（单一真相源在 settings.ts）
export type { LlmProvider, LlmProviderConfig } from '@/types/settings'

/** AI 上下文 */
export interface AiContext {
  sessionId?: string
  terminalOutput?: string
  includeContext: boolean
  /** 计划模式：先输出执行计划待用户确认，再逐步执行 */
  planMode?: boolean
}

/** 计划单步状态 */
export type PlanStepStatus = 'pending' | 'running' | 'done' | 'failed' | 'skipped'

/** 计划单步（从 ```plan 块编号行/列表行解析） */
export interface PlanStep {
  text: string
  status: PlanStepStatus
  /** 该步最近一次执行的命令（自动链完成 run 块时回填；重试直接复用） */
  command?: string
}

/** 计划整体状态机 */
export type PlanStatus = 'pending_confirm' | 'running' | 'paused_failed' | 'done' | 'cancelled'

/** 计划块的结构化状态（key: `${messageId}#${index}`，规则同 runStates） */
export interface PlanState {
  steps: PlanStep[]
  /** 当前执行到的步骤下标 */
  currentIndex: number
  status: PlanStatus
}

/** 命令分级：query=只读查询（自动执行）/ modify=修改类（确认或自动模式）/ danger=危险（始终手动二次确认） */
export type CommandLevel = 'query' | 'modify' | 'danger'

/** run 命令块的执行状态（key: `${messageId}#${index}`） */
export interface RunState {
  status: 'running' | 'done' | 'error' | 'rejected'
  output?: string
  /** 退出码（PTY 路径标记协议解析；非 0 时计划步骤判失败） */
  exitCode?: number
}

/** Agent 助手会话（持久化的一档对话，Chat 面板一个 Tab） */
export interface AgentChat {
  id: string
  /** 标题（空串 = 未命名，UI 显示「新会话」；首条用户消息自动生成） */
  title: string
  /** 关联终端会话的区分键（profileId，快速连接回退 tab 标题；空串 = 未关联/旧数据） */
  profileId: string
  createdAt: number
  updatedAt: number
}

/** 对话消息（UI 展示用） */
export interface ChatMessage {
  /** 消息 ID */
  id: string
  /** 角色（tool = 命令执行结果，仅前端展示层；发往 LLM 时作为 user 消息） */
  role: 'user' | 'assistant' | 'system' | 'tool'
  /** 内容 */
  content: string
  /** 时间戳 */
  timestamp: number
  /** 是否正在生成（assistant 消息专用） */
  pending?: boolean
  /** 是否出错 */
  error?: boolean
  /** 附加元数据 JSON（plan 块步骤状态机持久化载体） */
  meta?: string
}

/** AI token 事件 payload */
export interface AiTokenPayload {
  sessionId: string
  token: string
}

/** AI done 事件 payload */
export interface AiDonePayload {
  sessionId: string
  success: boolean
  error?: string
}
