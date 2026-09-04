/**
 * AI 助手命令组封装
 */

import { invoke } from './invoke'
import type { LlmProviderConfig, AiContext } from '@/types/ai'

/** 发送消息（流式响应通过事件推送） */
export async function chatSend(
  sessionId: string,
  message: string,
  ctx: AiContext,
): Promise<void> {
  return invoke<void>('ai_chat_send', { sessionId, message, ctx })
}

/** 中断当前生成 */
export async function chatAbort(): Promise<void> {
  return invoke<void>('ai_chat_abort')
}

/** 清空指定会话的对话历史 */
export async function chatClear(sessionId: string): Promise<void> {
  return invoke<void>('ai_chat_clear', { sessionId })
}

/** 更新 LLM 配置 */
export async function updateConfig(config: LlmProviderConfig): Promise<void> {
  return invoke<void>('ai_update_config', { config })
}
