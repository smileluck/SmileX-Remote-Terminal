/**
 * Agent 助手会话命令组封装（多会话 Tab 持久化）
 */

import { invoke } from './invoke'
import type { AgentChat, ChatMessage } from '@/types/ai'

/** 持久化的会话消息 */
export interface AgentChatMessage {
  id: string
  chatId: string
  role: ChatMessage['role']
  content: string
  error: boolean
  createdAt: number
}

/** 全部会话（最近更新在前） */
export function chatList(): Promise<AgentChat[]> {
  return invoke<AgentChat[]>('agent_chat_list')
}

/** 新建会话 */
export function chatCreate(chat: AgentChat): Promise<void> {
  return invoke<void>('agent_chat_create', { chat })
}

/** 重命名会话 */
export function chatRename(chatId: string, title: string): Promise<void> {
  return invoke<void>('agent_chat_rename', { chatId, title })
}

/** 删除会话（级联删消息） */
export function chatDelete(chatId: string): Promise<boolean> {
  return invoke<boolean>('agent_chat_delete', { chatId })
}

/** 某会话全部消息（旧→新） */
export function chatMessages(chatId: string): Promise<AgentChatMessage[]> {
  return invoke<AgentChatMessage[]>('agent_chat_messages', { chatId })
}

/** 追加一条消息，返回更新后的会话（前端同步排序与标题） */
export function chatMessageAppend(message: AgentChatMessage): Promise<AgentChat> {
  return invoke<AgentChat>('agent_chat_message_append', { message })
}

/** 清空某会话的消息（保留会话本身） */
export function chatClearMessages(chatId: string): Promise<void> {
  return invoke<void>('agent_chat_clear_messages', { chatId })
}
