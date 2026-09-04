import { defineStore } from 'pinia'
import { ref, watch } from 'vue'

import * as aiService from '@/services/ai'
import * as sessionService from '@/services/session'
import { listen } from '@/services/invoke'
import { stripThink } from '@/utils/think'
import { useTabsStore } from '@/stores/tabs'
import type { ChatMessage, AiContext, AiTokenPayload, AiDonePayload, RunState } from '@/types/ai'

/** ```run 块解析（与 MessageBubble 的解析规则保持一致） */
const RUN_RE = /```run\s*\n([\s\S]*?)```/g

/** 危险命令特征：自动模式跳过，手动执行需二次确认 */
const DANGER_PATTERNS: RegExp[] = [
  /\brm\s+(-[a-z]*r[a-z]*f|-[a-z]*f[a-z]*r)/i, // rm -rf
  /\bmkfs(\.\w+)?\b/i,
  /\bdd\b/i,
  /\b(shutdown|reboot|halt|poweroff|init\s+[06])\b/i,
  /:\(\)\s*\{.*\};:/i, // fork bomb
  />\s*\/dev\/(sd|nvme|vd)/i, // 直写磁盘设备
  /\b(curl|wget)\b[^\n|]*\|\s*(sudo\s+)?(ba|z|da)?sh\b/i, // 下载内容直接进 shell
  /\bkill\s+-9\s+-1\b/i,
]

/** 自动执行模式持久化键 */
const AUTORUN_KEY = 'smilex-agent-autorun'
/** 单条命令回传 LLM 的输出截断上限 */
const MAX_EXEC_OUTPUT = 8 * 1024
/** 一次用户提问后自动执行的链路上限（防止 agent 循环失控） */
const MAX_AUTO_CHAIN = 8

/**
 * Agent 助手 store（全局单例）
 *
 * - 对话状态全局化：右栏页签切换/收起不丢对话（ai_token/ai_done 监听只注册一次）
 * - 绑定目标 SSH 会话：默认跟随激活的 SSH tab，失效时回退到任一活跃会话
 * - 命令执行协议：LLM 回复中的 ```run 块经用户确认（或自动模式）后
 *   通过 session_exec 在绑定的服务器上执行，结果回传对话继续分析
 */
export const useAgentStore = defineStore('agent', () => {
  const tabs = useTabsStore()

  /** Chat 会话 ID（ai_token/ai_done 事件过滤 + 后端历史绑定） */
  const chatSessionId = `chat-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`
  /** 消息列表 */
  const messages = ref<ChatMessage[]>([])
  /** 是否正在生成 */
  const loading = ref(false)
  /** 错误信息 */
  const error = ref<string | null>(null)
  /** 是否附带终端/监控上下文 */
  const includeContext = ref(true)
  /** 绑定的目标 SSH 会话（null = 未连接） */
  const sshSessionId = ref<string | null>(null)
  /** 自动执行模式（信任模式：命令不再逐条确认，危险命令仍跳过） */
  const autoRun = ref(localStorage.getItem(AUTORUN_KEY) === '1')
  /** run 块执行状态：`${messageId}#${index}` → RunState */
  const runStates = ref<Record<string, RunState>>({})

  watch(autoRun, (v) => {
    try {
      localStorage.setItem(AUTORUN_KEY, v ? '1' : '0')
    } catch {
      /* 存储不可用时忽略 */
    }
  })

  /** 自动执行链计数（每次用户发送/清空时重置） */
  let autoChain = 0

  /* ---------------- 事件监听（App 启动注册一次） ---------------- */
  let started = false
  async function setupListeners() {
    if (started) return
    started = true
    await listen<AiTokenPayload>('ai_token', (p) => {
      if (p.sessionId !== chatSessionId) return
      const last = messages.value[messages.value.length - 1]
      if (last && last.role === 'assistant' && last.pending) last.content += p.token
    })
    await listen<AiDonePayload>('ai_done', (p) => {
      if (p.sessionId !== chatSessionId) return
      loading.value = false
      const last = messages.value[messages.value.length - 1]
      if (last && last.role === 'assistant' && last.pending) {
        last.pending = false
        if (!p.success) {
          last.error = true
          last.content = p.error || '生成失败'
        }
      }
      // 自动模式：继续执行回复中的命令（形成 agent 循环，链路有上限）
      if (p.success && autoRun.value && last && last.role === 'assistant' && !last.error) {
        void autoStep(last)
      }
    })
  }
  void setupListeners().catch((e) => {
    // 事件监听注册失败（如 capability 未授权）会静默丢失 ai_token/ai_done，
    // 这里至少暴露到控制台，避免完全无感知
    console.error('[agent] 事件监听注册失败:', e)
  })

  /* ---------------- 会话绑定：跟随激活的 SSH tab ---------------- */
  watch(
    () => tabs.activeTab,
    (t) => {
      if (t?.kind === 'ssh' && t.sessionId && !t.disconnected) sshSessionId.value = t.sessionId
    },
  )
  /** 绑定的会话失效（tab 关闭/断开）时回退到任一活跃 SSH 会话 */
  watch(
    () => tabs.tabs,
    () => {
      const alive = tabs.tabs.some(
        (t) => t.sessionId === sshSessionId.value && !t.disconnected,
      )
      if (!alive) {
        sshSessionId.value =
          tabs.tabs.find((t) => t.kind === 'ssh' && t.sessionId && !t.disconnected)
            ?.sessionId ?? null
      }
    },
    { deep: true },
  )

  /** 危险命令检测 */
  function isDangerous(cmd: string): boolean {
    return DANGER_PATTERNS.some((re) => re.test(cmd))
  }

  /** 组装上下文（后端按 sessionId 注入 scrollback/指标/服务器身份） */
  function buildCtx(): AiContext {
    const use = includeContext.value && !!sshSessionId.value
    return { sessionId: use ? sshSessionId.value! : undefined, includeContext: use }
  }

  /** 推送到 LLM（占位 assistant 消息 + 流式事件回填） */
  async function llmSend(text: string) {
    if (loading.value) return
    messages.value.push({
      id: `a-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`,
      role: 'assistant',
      content: '',
      timestamp: Date.now(),
      pending: true,
    })
    loading.value = true
    error.value = null
    try {
      // 生成结果（含失败）由 ai_done 事件回填气泡
      await aiService.chatSend(chatSessionId, text, buildCtx())
    } catch (e) {
      // 兜底：invoke 本身失败（此场景不会有 ai_done 事件），错误只写入气泡，
      // 不设置 error.value 以免与气泡双重显示
      loading.value = false
      const msg = e instanceof Error ? e.message : String(e)
      const last = messages.value[messages.value.length - 1]
      if (last && last.role === 'assistant') {
        last.pending = false
        last.error = true
        last.content = msg
      }
    }
  }

  /** 用户发送消息 */
  async function send(text: string) {
    if (!text.trim() || loading.value) return
    autoChain = 0
    messages.value.push({
      id: `u-${Date.now()}`,
      role: 'user',
      content: text,
      timestamp: Date.now(),
    })
    await llmSend(text)
  }

  /** 命令执行结果：前端展示为 tool 气泡，发给 LLM 作为续问上下文 */
  async function sendResult(cmd: string, output: string) {
    messages.value.push({
      id: `t-${Date.now()}`,
      role: 'tool',
      content: `$ ${cmd}\n${output}`,
      timestamp: Date.now(),
    })
    await llmSend(`[命令执行结果]\n$ ${cmd}\n${output}`)
  }

  /** 执行消息中第 index 个 run 块命令（在绑定的 SSH 会话上） */
  async function executeRun(messageId: string, index: number, command: string) {
    const key = `${messageId}#${index}`
    const st = runStates.value[key]
    if (st && (st.status === 'running' || st.status === 'done')) return
    if (loading.value) return
    if (!sshSessionId.value) {
      error.value = '未连接 SSH 服务器，无法执行命令'
      return
    }
    runStates.value = { ...runStates.value, [key]: { status: 'running' } }
    try {
      let out = await sessionService.exec(sshSessionId.value, command)
      if (out.length > MAX_EXEC_OUTPUT) {
        out = out.slice(0, MAX_EXEC_OUTPUT) + '\n…（输出过长，已截断）'
      }
      runStates.value = { ...runStates.value, [key]: { status: 'done', output: out } }
      await sendResult(command, out || '（无输出）')
    } catch (e) {
      const msg = String(e)
      runStates.value = { ...runStates.value, [key]: { status: 'error', output: msg } }
      await sendResult(command, `执行失败：${msg}`)
    }
  }

  /** 自动模式：执行回复中第一条未执行的 run 命令（危险命令跳过，留给手动确认） */
  async function autoStep(message: ChatMessage) {
    if (autoChain >= MAX_AUTO_CHAIN) {
      error.value = `自动执行已达上限（${MAX_AUTO_CHAIN} 条），剩余命令请手动执行`
      return
    }
    // 思考段内的内容不参与命令解析
    const cmds = [...stripThink(message.content).matchAll(RUN_RE)].map((m) => m[1].trim())
    for (let i = 0; i < cmds.length; i++) {
      if (runStates.value[`${message.id}#${i}`]) continue
      if (isDangerous(cmds[i])) continue
      autoChain++
      await executeRun(message.id, i, cmds[i])
      // 执行结果触发新一轮生成，其 ai_done 会继续 autoStep
      return
    }
  }

  /** 中断生成 */
  async function abort() {
    await aiService.chatAbort().catch(() => {})
    loading.value = false
  }

  /** 清空对话与执行状态 */
  async function clear() {
    messages.value = []
    runStates.value = {}
    error.value = null
    autoChain = 0
    await aiService.chatClear().catch(() => {})
  }

  return {
    chatSessionId,
    messages,
    loading,
    error,
    includeContext,
    sshSessionId,
    autoRun,
    runStates,
    isDangerous,
    send,
    executeRun,
    abort,
    clear,
  }
})
