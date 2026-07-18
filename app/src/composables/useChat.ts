/**
 * useChat - AI Chat composable
 *
 * 封装对话状态 + 流式响应监听。
 */
import { ref, onUnmounted } from 'vue'

import * as aiService from '@/services/ai'
import { listen } from '@/services/invoke'
import type { ChatMessage, AiContext, AiTokenPayload, AiDonePayload } from '@/types/ai'

export function useChat() {
  /** 当前会话 ID（用于绑定 Tab） */
  const sessionId = ref<string>(`chat-${Date.now()}`)
  /** 消息列表 */
  const messages = ref<ChatMessage[]>([])
  /** 是否正在生成 */
  const loading = ref(false)
  /** 错误信息 */
  const error = ref<string | null>(null)
  /** 是否附带当前会话上下文 */
  const includeContext = ref(false)
  /** 当前活动 SSH 会话 ID（用于上下文采集，由父组件传入） */
  const sshSessionId = ref<string | null>(null)

  let unlistenToken: (() => void) | null = null
  let unlistenDone: (() => void) | null = null

  /** 初始化事件监听 */
  async function setupListeners() {
    unlistenToken = await listen<AiTokenPayload>('ai_token', (p) => {
      if (p.sessionId !== sessionId.value) return
      // 找到最后一条 pending 的 assistant 消息，追加 token
      const last = messages.value[messages.value.length - 1]
      if (last && last.role === 'assistant' && last.pending) {
        last.content += p.token
      }
    })

    unlistenDone = await listen<AiDonePayload>('ai_done', (p) => {
      if (p.sessionId !== sessionId.value) return
      loading.value = false
      const last = messages.value[messages.value.length - 1]
      if (last && last.role === 'assistant' && last.pending) {
        last.pending = false
        if (!p.success) {
          last.error = true
          last.content = p.error || '生成失败'
        }
      }
    })
  }

  /** 发送消息 */
  async function send(text: string) {
    if (!text.trim() || loading.value) return

    // 追加用户消息
    messages.value.push({
      id: `u-${Date.now()}`,
      role: 'user',
      content: text,
      timestamp: Date.now(),
    })

    // 占位 assistant 消息
    messages.value.push({
      id: `a-${Date.now()}`,
      role: 'assistant',
      content: '',
      timestamp: Date.now(),
      pending: true,
    })

    loading.value = true
    error.value = null

    // 上下文
    const ctx: AiContext = {
      sessionId: includeContext.value ? sshSessionId.value || undefined : undefined,
      includeContext: includeContext.value,
    }

    try {
      await aiService.chatSend(sessionId.value, text, ctx)
    } catch (e) {
      loading.value = false
      error.value = String(e)
      // 标记最后一条 assistant 出错
      const last = messages.value[messages.value.length - 1]
      if (last && last.role === 'assistant') {
        last.pending = false
        last.error = true
        last.content = error.value || '调用失败'
      }
    }
  }

  /** 中断生成 */
  async function abort() {
    await aiService.chatAbort().catch(() => {})
    loading.value = false
  }

  /** 清空历史 */
  async function clear() {
    messages.value = []
    await aiService.chatClear().catch(() => {})
  }

  // 自动初始化监听
  setupListeners()

  onUnmounted(() => {
    unlistenToken?.()
    unlistenDone?.()
  })

  return {
    sessionId,
    messages,
    loading,
    error,
    includeContext,
    sshSessionId,
    send,
    abort,
    clear,
  }
}
