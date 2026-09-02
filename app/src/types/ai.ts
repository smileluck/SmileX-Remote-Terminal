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
}

/** run 命令块的执行状态（key: `${messageId}#${index}`） */
export interface RunState {
  status: 'running' | 'done' | 'error'
  output?: string
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
