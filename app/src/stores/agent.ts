import { defineStore } from 'pinia'
import { ref, computed, watch } from 'vue'

import * as aiService from '@/services/ai'
import * as agentChatService from '@/services/agentChat'
import * as sessionService from '@/services/session'
import * as termExec from '@/services/termExec'
import { listen } from '@/services/invoke'
import { stripThink } from '@/utils/think'
import { useTabsStore } from '@/stores/tabs'
import type {
  AgentChat,
  ChatMessage,
  AiContext,
  AiTokenPayload,
  AiDonePayload,
  RunState,
} from '@/types/ai'

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

/** 生成前端消息 / 会话 ID（crypto.randomUUID 不可用时回退随机串） */
function genId(prefix: string): string {
  const rnd =
    typeof crypto !== 'undefined' && 'randomUUID' in crypto
      ? crypto.randomUUID().slice(0, 8)
      : Math.random().toString(36).slice(2, 10)
  return `${prefix}-${Date.now()}-${rnd}`
}

/**
 * Agent 助手 store（全局单例）
 *
 * - 历史会话：对话自动按会话持久化到 SQLite，重启可恢复；
 *   后端 LLM 上下文按会话隔离，切回历史会话自动从库恢复。
 *   单会话推进：新会话仅经「新会话」动作创建（首条消息也会兜底创建），
 *   无多开入口
 * - 事件路由：后端 sessionId 即会话 id，ai_token/ai_done 按其分发到
 *   对应会话的消息数组，生成中切换会话流式不丢
 * - 绑定目标 SSH 会话：默认跟随激活的 SSH tab，失效时回退到任一活跃会话
 * - 命令执行协议：LLM 回复中的 ```run 块经用户确认（或自动模式）后
 *   写入绑定的终端窗口会话执行（对用户可见、保留会话状态），
 *   从会话回显捕获输出回传对话继续分析；写入失败回退独立 exec 通道
 */
export const useAgentStore = defineStore('agent', () => {
  const tabs = useTabsStore()

  /** 会话列表（最近更新在前） */
  const chats = ref<AgentChat[]>([])
  /** 当前激活的会话 id */
  const activeChatId = ref<string | null>(null)
  /** 各会话消息（内存缓存，切换时按需从库加载） */
  const messagesByChat = ref<Record<string, ChatMessage[]>>({})
  /** 正在生成的会话（后端单生成，同一时刻至多一个） */
  const generatingChatId = ref<string | null>(null)
  /** 当前会话的消息（UI 绑定用） */
  const messages = computed(() =>
    activeChatId.value ? (messagesByChat.value[activeChatId.value] ?? []) : [],
  )
  /** 当前会话是否正在生成（控制停止按钮） */
  const loading = computed(
    () => !!activeChatId.value && generatingChatId.value === activeChatId.value,
  )
  /** 是否任一会话正在生成（后端单生成，控制输入禁用） */
  const busy = computed(() => generatingChatId.value !== null)
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

  /* ---------------- 会话管理 ---------------- */

  /** 取某会话的消息数组（无则建空数组，保证流式事件有处可写） */
  function msgListOf(chatId: string): ChatMessage[] {
    let list = messagesByChat.value[chatId]
    if (!list) {
      list = []
      messagesByChat.value[chatId] = list
    }
    return list
  }

  /** 从库加载某会话的消息（切换 Tab / 启动激活时） */
  async function loadMessages(chatId: string) {
    try {
      const rows = await agentChatService.chatMessages(chatId)
      messagesByChat.value[chatId] = rows.map((r) => ({
        id: r.id,
        role: r.role,
        content: r.content,
        timestamp: r.createdAt,
        ...(r.error ? { error: true } : {}),
      }))
    } catch {
      messagesByChat.value[chatId] = []
    }
  }

  /** 会话 upsert 到列表头（保持 updated_at 倒序） */
  function upsertChat(chat: AgentChat) {
    chats.value = [chat, ...chats.value.filter((c) => c.id !== chat.id)]
  }

  /** 启动初始化：拉取会话列表并激活最近一个（失败静默，可对话不落库） */
  let inited = false
  async function init() {
    if (inited) return
    inited = true
    try {
      const list = await agentChatService.chatList()
      chats.value = list
      if (list.length > 0) {
        activeChatId.value = list[0].id
        await loadMessages(list[0].id)
      }
    } catch {
      /* 非 Tauri 环境 */
    }
  }
  void init()

  /** 开始新会话并激活（当前对话保留在历史；当前会话尚无消息时直接复用） */
  async function newChat(): Promise<string> {
    const current = activeChatId.value
    if (current && (messagesByChat.value[current]?.length ?? 0) === 0) return current
    const id = genId('chat')
    const now = Math.floor(Date.now() / 1000)
    const chat: AgentChat = { id, title: '', createdAt: now, updatedAt: now }
    try {
      await agentChatService.chatCreate(chat)
      upsertChat(chat)
    } catch {
      /* 持久化不可用时仍可内存对话 */
    }
    messagesByChat.value[id] = []
    activeChatId.value = id
    error.value = null
    return id
  }

  /** 切换会话（按需加载消息） */
  async function switchChat(chatId: string) {
    if (activeChatId.value === chatId) return
    activeChatId.value = chatId
    if (!messagesByChat.value[chatId]) await loadMessages(chatId)
  }

  /** 删除会话（级联删消息；删激活会话自动切到最近一个） */
  async function deleteChat(chatId: string) {
    if (generatingChatId.value === chatId) {
      void aiService.chatAbort().catch(() => {})
      generatingChatId.value = null
    }
    try {
      await agentChatService.chatDelete(chatId)
    } catch {
      /* 忽略，仍移除本地 */
    }
    chats.value = chats.value.filter((c) => c.id !== chatId)
    delete messagesByChat.value[chatId]
    if (activeChatId.value === chatId) {
      activeChatId.value = chats.value[0]?.id ?? null
      if (activeChatId.value) await loadMessages(activeChatId.value)
    }
  }

  /* ---------------- 事件监听（App 启动注册一次） ---------------- */
  let started = false
  async function setupListeners() {
    if (started) return
    started = true
    // p.sessionId 即会话 id：增量写入对应会话的 pending assistant 消息，
    // 生成中切换 Tab 也不丢
    await listen<AiTokenPayload>('ai_token', (p) => {
      const list = messagesByChat.value[p.sessionId]
      const last = list?.[list.length - 1]
      if (last && last.role === 'assistant' && last.pending) last.content += p.token
    })
    await listen<AiDonePayload>('ai_done', (p) => {
      if (generatingChatId.value !== p.sessionId) return
      generatingChatId.value = null
      const chatId = p.sessionId
      const last = messagesByChat.value[chatId]?.at(-1)
      if (last && last.role === 'assistant' && last.pending) {
        last.pending = false
        if (!p.success) {
          last.error = true
          last.content = p.error || '生成失败'
        }
      }
      // 定稿落库（失败占位也存，恢复上下文时后端会跳过）
      if (last && last.role === 'assistant') persist(chatId, last)
      // 自动模式：继续执行回复中的命令（形成 agent 循环，链路有上限）
      if (p.success && autoRun.value && last && last.role === 'assistant' && !last.error) {
        void autoStep(chatId, last)
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

  /** 消息落库（fire-and-forget；返回的会话用于同步列表排序与标题） */
  function persist(chatId: string, msg: ChatMessage) {
    if (msg.role === 'assistant' && msg.pending) return
    agentChatService
      .chatMessageAppend({
        id: msg.id,
        chatId,
        role: msg.role,
        content: msg.content,
        error: !!msg.error,
        createdAt: msg.timestamp,
      })
      .then(upsertChat)
      .catch(() => {})
  }

  /** 推送到 LLM（占位 assistant 消息 + 流式事件回填） */
  async function llmSend(chatId: string, text: string) {
    if (generatingChatId.value) return
    msgListOf(chatId).push({
      id: genId('a'),
      role: 'assistant',
      content: '',
      timestamp: Date.now(),
      pending: true,
    })
    generatingChatId.value = chatId
    error.value = null
    try {
      // 生成结果（含失败）由 ai_done 事件回填气泡
      await aiService.chatSend(chatId, text, buildCtx())
    } catch (e) {
      // 兜底：invoke 本身失败（此场景不会有 ai_done 事件），错误只写入气泡，
      // 不设置 error.value 以免与气泡双重显示
      generatingChatId.value = null
      const msg = e instanceof Error ? e.message : String(e)
      const list = msgListOf(chatId)
      const last = list[list.length - 1]
      if (last && last.role === 'assistant') {
        last.pending = false
        last.error = true
        last.content = msg
        persist(chatId, last)
      }
    }
  }

  /** 用户发送消息 */
  async function send(text: string) {
    if (!text.trim() || generatingChatId.value) return
    const chatId = activeChatId.value ?? (await newChat())
    autoChain = 0
    const list = msgListOf(chatId)
    list.push({
      id: genId('u'),
      role: 'user',
      content: text,
      timestamp: Date.now(),
    })
    persist(chatId, list[list.length - 1])
    await llmSend(chatId, text)
  }

  /** 命令执行结果：不回显到面板（ChatPanel 过滤 tool 消息），发给 LLM 作为续问上下文 */
  async function sendResult(chatId: string, cmd: string, output: string) {
    const list = msgListOf(chatId)
    list.push({
      id: genId('t'),
      role: 'tool',
      content: `$ ${cmd}\n${output}`,
      timestamp: Date.now(),
    })
    persist(chatId, list[list.length - 1])
    await llmSend(chatId, `[命令执行结果]\n$ ${cmd}\n${output}`)
  }

  /** 定位消息所属会话（执行结果须回到发起会话，即使已切换 Tab） */
  function findChatIdOf(messageId: string): string | null {
    for (const [chatId, list] of Object.entries(messagesByChat.value)) {
      if (list.some((m) => m.id === messageId)) return chatId
    }
    return null
  }

  /** 执行消息中第 index 个 run 块命令（在绑定的 SSH 会话上） */
  async function executeRun(messageId: string, index: number, command: string) {
    const chatId = findChatIdOf(messageId) ?? activeChatId.value
    if (!chatId) return
    const key = `${messageId}#${index}`
    const st = runStates.value[key]
    if (st && (st.status === 'running' || st.status === 'done')) return
    if (generatingChatId.value) return
    if (!sshSessionId.value) {
      error.value = '未连接 SSH 服务器，无法执行命令'
      return
    }
    runStates.value = { ...runStates.value, [key]: { status: 'running' } }
    try {
      // 命令写入绑定的终端窗口会话（对用户可见、保留 cwd/env 状态），
      // 从会话回显捕获输出；写入失败（会话异常）回退独立 exec 通道
      let out: string
      try {
        out = await termExec.execInTerminal(sshSessionId.value, command)
      } catch {
        out = await sessionService.exec(sshSessionId.value, command)
      }
      if (out.length > MAX_EXEC_OUTPUT) {
        out = out.slice(0, MAX_EXEC_OUTPUT) + '\n…（输出过长，已截断）'
      }
      runStates.value = { ...runStates.value, [key]: { status: 'done', output: out } }
      await sendResult(chatId, command, out || '（无输出）')
    } catch (e) {
      const msg = String(e)
      runStates.value = { ...runStates.value, [key]: { status: 'error', output: msg } }
      await sendResult(chatId, command, `执行失败：${msg}`)
    }
  }

  /** 自动模式：执行回复中第一条未执行的 run 命令（危险命令跳过，留给手动确认） */
  async function autoStep(chatId: string, message: ChatMessage) {
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

  /** 中断当前生成（本地立即定稿已流式到达的内容） */
  async function abort() {
    const chatId = generatingChatId.value
    await aiService.chatAbort().catch(() => {})
    generatingChatId.value = null
    if (!chatId) return
    const last = messagesByChat.value[chatId]?.at(-1)
    if (last && last.role === 'assistant' && last.pending) {
      last.pending = false
      if (!last.content) last.content = '已中断'
      persist(chatId, last)
    }
  }

  return {
    chats,
    activeChatId,
    messages,
    busy,
    loading,
    error,
    includeContext,
    sshSessionId,
    autoRun,
    runStates,
    isDangerous,
    newChat,
    switchChat,
    deleteChat,
    send,
    executeRun,
    abort,
  }
})
