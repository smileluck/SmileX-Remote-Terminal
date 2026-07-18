/**
 * AI 模块类型定义
 */

/** LLM Provider 类型 */
export type LlmProvider = 'openai' | 'claude' | 'ollama'

/** LLM 配置 */
export interface LlmProviderConfig {
  provider: LlmProvider
  model: string
  baseUrl?: string
  apiKey?: string
  stream: boolean
}

/** AI 上下文 */
export interface AiContext {
  sessionId?: string
  terminalOutput?: string
  includeContext: boolean
}

/** 对话消息（UI 展示用） */
export interface ChatMessage {
  /** 消息 ID */
  id: string
  /** 角色 */
  role: 'user' | 'assistant' | 'system'
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
