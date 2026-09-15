import { defineStore } from 'pinia'
import { ref, computed, watch } from 'vue'

import * as aiService from '@/services/ai'
import * as aiExecService from '@/services/aiExec'
import * as agentChatService from '@/services/agentChat'
import * as termExec from '@/services/termExec'
import { listen } from '@/services/invoke'
import { stripThink } from '@/utils/think'
import { useTabsStore } from '@/stores/tabs'
import type { CommandRisk, ExecSource } from '@/services/aiExec'
import type {
  AgentChat,
  ChatMessage,
  AiContext,
  AiTokenPayload,
  AiDonePayload,
  CommandLevel,
  PlanState,
  PlanStep,
  RunState,
} from '@/types/ai'

/** ```run 块解析（与 MessageBubble 的解析规则保持一致） */
const RUN_RE = /```run\s*\n([\s\S]*?)```/g

/** ```plan 块解析（计划模式；与 MessageBubble 的解析规则保持一致） */
const PLAN_RE = /```plan\s*\n([\s\S]*?)```/g

/** 后端风险等级 → 前端三级展示 */
function riskToLevel(risk: CommandRisk): CommandLevel {
  return risk === 'read_only' ? 'query' : risk
}

/** 自动执行模式持久化键 */
const AUTORUN_KEY = 'smilex-agent-autorun'
/** 计划模式持久化键 */
const PLANMODE_KEY = 'smilex-agent-planmode'
/** 单条命令回传 LLM 的输出截断上限（按字节保尾截断） */
const MAX_EXEC_OUTPUT = 8 * 1024
/** 一次用户提问后自动执行的链路上限（防止 agent 循环失控） */
const MAX_AUTO_CHAIN = 8
/** LLM 流式输出累计上限（字节）：超过自动中止生成 */
const MAX_STREAM_BYTES = 256 * 1024

/** 按字节保尾截断（报错通常在输出末尾），截断时加前缀标记 */
const outputEncoder = new TextEncoder()
const outputDecoder = new TextDecoder('utf-8')
function truncateOutput(s: string): string {
  const bytes = outputEncoder.encode(s)
  if (bytes.length <= MAX_EXEC_OUTPUT) return s
  const dropped = bytes.length - MAX_EXEC_OUTPUT
  return `[已截断 ${dropped} 字节]\n${outputDecoder.decode(bytes.subarray(dropped))}`
}

/** 后端闸门 reason → 中文文案 */
const REJECT_REASON_TEXT: Record<string, string> = {
  chat_not_found: '会话不存在（可能已被删除）',
  chat_not_bound: '该会话未绑定服务器，无法执行命令',
  session_disconnected: '该会话绑定的服务器已断开，无法执行命令',
  danger_requires_confirmation: '危险命令需手动确认后执行',
  not_in_allowlist: '修改类命令未在授权白名单中',
}

function rejectReasonText(reason?: string): string {
  if (!reason) return '命令被安全闸门拒绝'
  return REJECT_REASON_TEXT[reason] ?? reason
}

/** ai_done 错误文案中文化（后端错误多为中文，常见英文模式翻译兜底） */
function humanizeLlmError(msg: string): string {
  if (!msg) return '生成失败'
  if (/timed?\s*out|timeout/i.test(msg)) return '请求超时，请稍后重试'
  if (/401|unauthorized|invalid\s*api\s*key/i.test(msg)) return 'API Key 无效或未授权（401）'
  if (/429|rate\s*limit/i.test(msg)) return '请求过于频繁，触发限流（429）'
  if (/\b5\d\d\b/.test(msg)) return '服务端错误，请稍后重试'
  if (/network|connection|refused|dns|resolve/i.test(msg)) return '网络连接失败，请检查网络或 Base URL'
  return msg
}

/** 解析 ```plan 块文本为结构化步骤（编号行 `1.` / 列表行 `- `；无匹配时整段作为单步） */
export function parsePlanSteps(planText: string): PlanStep[] {
  const steps: PlanStep[] = []
  for (const line of planText.split('\n')) {
    const m = line.match(/^\s*(?:\d+\s*[.、)]|[-*•])\s*(.+)$/)
    if (m) steps.push({ text: m[1].trim(), status: 'pending' })
  }
  if (steps.length === 0) steps.push({ text: planText.trim(), status: 'pending' })
  return steps
}

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
 *   聊天记录按终端会话区分（profileId，快速连接回退 tab 标题），
 *   切换标签自动跟随到该服务器最近的聊天；无聊天时显示空态、发送时懒创建。
 *   单会话推进：新会话仅经「新会话」动作创建（首条消息也会兜底创建），
 *   无多开入口
 * - 事件路由：后端 sessionId 即会话 id，ai_token/ai_done 按其分发到
 *   对应会话的消息数组，生成中切换会话流式不丢
 * - 上下文/执行目标绑定：聊天按区分键（profileId）绑定到所属服务器，
 *   上下文注入与命令执行均解析该聊天的绑定会话（而非跟随当前激活 tab），
 *   绑定的服务器断开后阻止执行并提示，绝不静默打到别的机器；
 *   仅无绑定的通用聊天回退到当前激活的 SSH 会话
 * - 计划模式（planMode）：开启后 AI 先输出 ```plan 块执行计划，用户确认后
 *   前端回传「计划已确认 + 计划原文」，AI 再按计划逐步输出 run 块执行；
 *   计划进度为结构化步骤状态机（planStates），逐步标记 done/failed，
 *   失败暂停（重试 / 跳过 / 终止），状态持久化到消息 meta
 * - 命令执行决策全部走后端安全闸门（ai_exec_prepare）：
 *   前端不再做安全分类正则，run 块的风险标签用 ai_classify_command
 *   异步查询 + 缓存，仅作展示；闸门放行后默认写绑定终端窗口会话执行
 *   （对用户可见、保留会话状态），完成后 ai_exec_finish 补记审计；
 *   写入失败回退 ai_exec_command 非交互通道（自带闸门 + 审计 + 截断）
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
  /** 当前终端会话的聊天区分键（profileId 优先，快速连接回退 tab 标题；非 SSH/已断开/无 tab = null，不切换聊天） */
  const currentKey = computed(() => {
    const t = tabs.activeTab
    if (t?.kind === 'ssh' && t.sessionId && !t.disconnected) return t.profileId || t.title
    return null
  })
  /** 自动执行模式（信任模式：修改类命令不再逐条确认；查询类始终自动执行，危险命令仍跳过） */
  const autoRun = ref(localStorage.getItem(AUTORUN_KEY) === '1')
  /** 计划模式（先出执行计划，用户确认后再逐步执行） */
  const planMode = ref(localStorage.getItem(PLANMODE_KEY) === '1')
  /** run 块执行状态：`${messageId}#${index}` → RunState */
  const runStates = ref<Record<string, RunState>>({})
  /** plan 块结构化状态机：`${messageId}#${index}` → PlanState（无记录 = pending_confirm 待确认） */
  const planStates = ref<Record<string, PlanState>>({})
  /** run 块风险标签缓存（ai_classify_command 异步查询结果，仅作展示；执行决策以闸门为准） */
  const riskCache = ref<Record<string, CommandLevel>>({})
  /** 正在查询风险等级的命令（防并发重复查询） */
  const riskInflight = new Set<string>()

  /** 修改类命令待确认（执行链路挂起点，由 ChatPanel 呈现确认框并回调） */
  const pendingConfirm = ref<{
    messageId: string
    index: number
    command: string
    /** 所属会话与绑定档案（「记住授权」白名单 scope 用；profileId 空 = 仅可记住到会话） */
    chatId: string
    profileId: string
    resolve: (choice: { ok: boolean; remember?: 'chat' | 'profile' }) => void
  } | null>(null)

  /** 用户确认/跳过修改类命令（ChatPanel 确认框回调） */
  function resolvePendingConfirm(choice: { ok: boolean; remember?: 'chat' | 'profile' }) {
    const p = pendingConfirm.value
    pendingConfirm.value = null
    p?.resolve(choice)
  }

  /** 流式输出累计字节数（LLM→前端方向保护，超过 MAX_STREAM_BYTES 自动中止） */
  const streamBytes = new Map<string, number>()
  /** 已因超限中止的会话（ai_done 据此分类 limit） */
  const streamLimited = new Set<string>()

  watch(autoRun, (v) => {
    try {
      localStorage.setItem(AUTORUN_KEY, v ? '1' : '0')
    } catch {
      /* 存储不可用时忽略 */
    }
  })

  watch(planMode, (v) => {
    try {
      localStorage.setItem(PLANMODE_KEY, v ? '1' : '0')
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

  /** 从库加载某会话的消息（切换 Tab / 启动激活时）；还原消息 meta 中的 plan 步骤状态机 */
  async function loadMessages(chatId: string) {
    try {
      const rows = await agentChatService.chatMessages(chatId)
      messagesByChat.value[chatId] = rows.map((r) => ({
        id: r.id,
        role: r.role,
        content: r.content,
        timestamp: r.createdAt,
        ...(r.error ? { error: true } : {}),
        ...(r.meta ? { meta: r.meta } : {}),
      }))
      // 还原持久化的 plan 状态（运行中的计划重启后按已暂停处理，避免自动续跑）
      for (const r of rows) {
        if (!r.meta) continue
        try {
          const meta = JSON.parse(r.meta) as { planStates?: Record<string, PlanState> }
          for (const [idx, plan] of Object.entries(meta.planStates ?? {})) {
            const restored: PlanState =
              plan.status === 'running'
                ? {
                    ...plan,
                    status: 'paused_failed',
                    steps: plan.steps.map((s) =>
                      s.status === 'running' ? { ...s, status: 'failed' as const } : s,
                    ),
                  }
                : plan
            planStates.value = { ...planStates.value, [`${r.id}#${idx}`]: restored }
          }
        } catch {
          /* meta 非 JSON / 结构不符时忽略 */
        }
      }
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
      // 启动时已有激活的 SSH tab：按区分键校正激活会话
      await syncChatWithKey()
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
    const chat: AgentChat = { id, title: '', profileId: currentKey.value ?? '', createdAt: now, updatedAt: now }
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
    // 清理后端 LLM 内存中的该会话历史
    void aiService.chatClear(chatId).catch(() => {})
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

  /**
   * 清空会话消息（保留会话本身）：SQLite、后端 LLM 内存历史、本地缓存三处
   * 同步清理——后端 seed_history 是幂等的 or_insert，只清库不清内存会残留旧上下文
   */
  async function clearMessages(chatId: string) {
    void aiService.chatClear(chatId).catch(() => {})
    try {
      await agentChatService.chatClearMessages(chatId)
    } catch {
      /* 持久化不可用时仍清本地 */
    }
    messagesByChat.value[chatId] = []
  }

  /* ---------------- 事件监听（App 启动注册一次） ---------------- */
  let started = false
  async function setupListeners() {
    if (started) return
    started = true
    // p.sessionId 即会话 id：增量写入对应会话的 pending assistant 消息，
    // 生成中切换 Tab 也不丢；累计超过 256KB 自动中止（防输出失控撑爆 UI）
    await listen<AiTokenPayload>('ai_token', (p) => {
      const list = messagesByChat.value[p.sessionId]
      const last = list?.[list.length - 1]
      if (!last || last.role !== 'assistant' || !last.pending) return
      if (streamLimited.has(p.sessionId)) return
      const n = (streamBytes.get(p.sessionId) ?? 0) + outputEncoder.encode(p.token).length
      streamBytes.set(p.sessionId, n)
      if (n > MAX_STREAM_BYTES) {
        streamLimited.add(p.sessionId)
        last.content += '\n\n⚠ 输出超限已中止'
        void aiService.chatAbort().catch(() => {})
        return
      }
      last.content += p.token
    })
    await listen<AiDonePayload>('ai_done', (p) => {
      if (generatingChatId.value !== p.sessionId) return
      generatingChatId.value = null
      const chatId = p.sessionId
      const limited = streamLimited.delete(chatId)
      streamBytes.delete(chatId)
      const last = messagesByChat.value[chatId]?.at(-1)
      if (last && last.role === 'assistant' && last.pending) {
        last.pending = false
        if (!p.success) {
          const errText = p.error || ''
          if (limited || errText.includes('输出超限')) {
            // 输出超限中止：保留已生成内容，标记错误样式
            last.error = true
            if (!last.content.includes('输出超限')) last.content += '\n\n⚠ 输出超限已中止'
          } else if (errText.includes('操作已取消')) {
            // 用户主动取消：安静收尾，不显示错误样式
            if (!last.content) last.content = '已取消'
          } else {
            last.error = true
            last.content = humanizeLlmError(errText)
          }
        }
      }
      // 定稿落库（失败占位也存，恢复上下文时后端会跳过）
      if (last && last.role === 'assistant') persist(chatId, last)
      // 自动链路：查询类命令始终自动执行；修改类仅在自动模式直接执行，
      // 否则弹窗逐条确认；危险命令跳过留给手动（链路有上限）
      if (p.success && last && last.role === 'assistant' && !last.error) {
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
  /** 聊天跟随终端会话切换：区分键变化时切到该 key 下最近的聊天（无则空态，不自动建聊天） */
  async function syncChatWithKey() {
    const key = currentKey.value
    if (!key) return
    const active = chats.value.find((c) => c.id === activeChatId.value)
    if (active?.profileId === key) return
    // chats 按 updatedAt 倒序，首个匹配即该 key 下最近的聊天
    const next = chats.value.find((c) => c.profileId === key)
    activeChatId.value = next?.id ?? null
    if (next && !messagesByChat.value[next.id]) await loadMessages(next.id)
  }

  watch(
    () => tabs.activeTab,
    (t) => {
      if (t?.kind === 'ssh' && t.sessionId && !t.disconnected) sshSessionId.value = t.sessionId
      void syncChatWithKey()
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

  /** run 块风险标签（渲染用）：命中缓存直接返回，未命中触发异步查询后响应式更新（null = 查询中） */
  function riskOf(cmd: string): CommandLevel | null {
    const hit = riskCache.value[cmd]
    if (hit) return hit
    void ensureRisk(cmd)
    return null
  }

  /** 查询命令风险等级（缓存优先；分类服务不可用时按修改类兜底——仅影响展示与预检交互，执行决策以闸门为准） */
  async function ensureRisk(cmd: string): Promise<CommandLevel> {
    const hit = riskCache.value[cmd]
    if (hit) return hit
    if (riskInflight.has(cmd)) {
      // 已有查询在途：轮询等待其结果
      for (let i = 0; i < 100 && riskInflight.has(cmd); i++) {
        await new Promise((r) => setTimeout(r, 50))
      }
      return riskCache.value[cmd] ?? 'modify'
    }
    riskInflight.add(cmd)
    try {
      const level = riskToLevel(await aiExecService.classifyCommand(cmd))
      riskCache.value = { ...riskCache.value, [cmd]: level }
      return level
    } catch {
      return 'modify'
    } finally {
      riskInflight.delete(cmd)
    }
  }

  /**
   * 解析聊天绑定的目标 SSH 会话（上下文注入与命令执行的唯一目标）
   * - 聊天有区分键（profileId）：找该键下活跃的 SSH tab；找不到 → bound=true（阻止执行）
   * - 无绑定（通用聊天）：回退当前激活的 SSH 会话
   */
  function resolveSessionForChat(chatId: string): { sessionId: string | null; bound: boolean } {
    const key = chats.value.find((c) => c.id === chatId)?.profileId
    if (!key) return { sessionId: sshSessionId.value, bound: false }
    const tab = tabs.tabs.find(
      (t) => t.kind === 'ssh' && t.sessionId && !t.disconnected && (t.profileId || t.title) === key,
    )
    return { sessionId: tab?.sessionId ?? null, bound: !tab }
  }

  /** 执行前置检查：返回不可执行的原因（null = 可执行） */
  function execBlockReason(messageId: string): string | null {
    const chatId = findChatIdOf(messageId) ?? activeChatId.value
    if (!chatId) return null
    const { sessionId, bound } = resolveSessionForChat(chatId)
    if (sessionId) return null
    return bound ? '该会话绑定的服务器已断开，无法执行命令' : '未连接 SSH 服务器，请先连接'
  }

  /** 组装上下文（后端按 sessionId 注入 scrollback/指标/服务器身份） */
  function buildCtx(chatId: string): AiContext {
    const { sessionId } = resolveSessionForChat(chatId)
    const use = includeContext.value && !!sessionId
    return {
      sessionId: use ? sessionId! : undefined,
      includeContext: use,
      planMode: planMode.value,
    }
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
        meta: msg.meta,
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
      await aiService.chatSend(chatId, text, buildCtx(chatId))
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
    // 用户接管对话：取消挂起的修改类命令确认（其链路随之停止）
    if (pendingConfirm.value) resolvePendingConfirm({ ok: false })
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

  /** 执行消息中第 index 个 run 块命令（在该聊天绑定的 SSH 会话上，全路过后端闸门） */
  async function executeRun(
    messageId: string,
    index: number,
    command: string,
    opts: { approved?: boolean; source?: ExecSource } = {},
  ) {
    const chatId = findChatIdOf(messageId) ?? activeChatId.value
    if (!chatId) return
    const key = `${messageId}#${index}`
    const st = runStates.value[key]
    if (st && (st.status === 'running' || st.status === 'done')) return
    if (generatingChatId.value) return
    const { sessionId, bound } = resolveSessionForChat(chatId)
    if (!sessionId) {
      error.value = bound
        ? '该会话绑定的服务器已断开，无法执行命令'
        : '未连接 SSH 服务器，请先连接'
      return
    }
    await runGated(chatId, key, command, opts.source ?? 'manual', !!opts.approved)
  }

  /**
   * 经后端闸门执行一条命令的核心流程（run 块与计划重试共用）
   *
   * 时序：ai_exec_prepare(approved) → rejected 展示原因 / needs_approval 弹窗
   * （确认后 approved=true 重新 prepare，勾选「记住授权」同时写白名单）→
   * allowed 后 PTY 可见路径执行，完成 ai_exec_finish 补记审计；
   * PTY 写入失败回退 ai_exec_command（自带闸门 + 审计 + 截断）。
   *
   * @returns 执行是否成功（输出回传与计划推进由内部完成）
   */
  async function runGated(
    chatId: string,
    runKey: string,
    command: string,
    source: ExecSource,
    approved: boolean,
  ): Promise<boolean> {
    const sessionId = sessionIdOf(chatId)
    if (!sessionId) return false
    // 1) 后端闸门预检（所有执行决策以闸门结果为准）
    let prep: aiExecService.ExecPrepareResult
    try {
      prep = await aiExecService.execPrepare(chatId, command, source, approved)
    } catch (e) {
      runStates.value = { ...runStates.value, [runKey]: { status: 'error', output: String(e) } }
      error.value = `执行预检失败：${String(e)}`
      return false
    }
    if (prep.status === 'rejected') {
      const text = rejectReasonText(prep.reason)
      runStates.value = { ...runStates.value, [runKey]: { status: 'rejected', output: text } }
      error.value = text
      return false
    }
    if (prep.status === 'needs_approval') {
      // 挂起链路，弹窗询问（danger 不会走到这里：UI 二次确认时已带 approved=true）
      const profileId = chats.value.find((c) => c.id === chatId)?.profileId ?? ''
      const messageId = runKey.slice(0, runKey.indexOf('#'))
      const idxStr = runKey.slice(runKey.indexOf('#') + 1)
      const choice = await new Promise<{ ok: boolean; remember?: 'chat' | 'profile' }>(
        (resolve) => {
          pendingConfirm.value = {
            messageId,
            index: Number.parseInt(idxStr, 10) || 0,
            command,
            chatId,
            profileId,
            resolve,
          }
        },
      )
      if (!choice.ok) return false // 用户跳过：不执行（自动链路随之停止）
      approved = true
      if (choice.remember) {
        const scopeId = choice.remember === 'chat' ? chatId : profileId
        void aiExecService
          .allowlistAdd(command.trim(), prep.risk, choice.remember, scopeId)
          .catch(() => {})
      }
      // 用户确认后带 approved=true 重新过闸
      try {
        prep = await aiExecService.execPrepare(chatId, command, source, true)
      } catch (e) {
        runStates.value = { ...runStates.value, [runKey]: { status: 'error', output: String(e) } }
        error.value = `执行预检失败：${String(e)}`
        return false
      }
      if (prep.status !== 'allowed') {
        const text = rejectReasonText(prep.reason)
        runStates.value = { ...runStates.value, [runKey]: { status: 'rejected', output: text } }
        error.value = text
        return false
      }
    }

    // 2) 执行：默认 PTY 可见路径，完成后补记审计；写入失败回退非交互通道
    runStates.value = { ...runStates.value, [runKey]: { status: 'running' } }
    const started = Date.now()
    try {
      let out: string
      let exitCode: number | undefined
      try {
        const r = await termExec.execInTerminalDetailed(sessionId, command)
        out = r.text
        exitCode = r.rc
        void aiExecService
          .execFinish(chatId, command, prep.risk, source, exitCode, Date.now() - started)
          .catch(() => {})
      } catch {
        const res = await aiExecService.execCommand(chatId, command, source, approved)
        if (res.status !== 'executed') throw new Error(res.error ?? '执行失败')
        out = res.output ?? ''
      }
      out = truncateOutput(out)
      runStates.value = { ...runStates.value, [runKey]: { status: 'done', output: out, exitCode } }
      await sendResult(chatId, command, out || '（无输出）')
      const ok = exitCode === undefined || exitCode === 0
      advancePlan(chatId, ok, command)
      return ok
    } catch (e) {
      const msg = String(e)
      runStates.value = { ...runStates.value, [runKey]: { status: 'error', output: msg } }
      await sendResult(chatId, command, `执行失败：${msg}`)
      advancePlan(chatId, false, command)
      return false
    }
  }

  /** 取聊天当前解析到的 SSH 会话 id（执行时取，保证与预检同源） */
  function sessionIdOf(chatId: string): string | null {
    return resolveSessionForChat(chatId).sessionId
  }

  /* ---------------- 计划步骤状态机 ---------------- */

  /** 更新 plan 状态（响应式）并序列化进所属消息的 meta 持久化 */
  function setPlanState(key: string, plan: PlanState) {
    planStates.value = { ...planStates.value, [key]: plan }
    persistPlanMeta(key)
  }

  /** 把某消息全部 plan 块状态序列化进该消息的 meta 列 */
  function persistPlanMeta(key: string) {
    const messageId = key.slice(0, key.lastIndexOf('#'))
    const chatId = findChatIdOf(messageId)
    if (!chatId) return
    const msg = messagesByChat.value[chatId]?.find((m) => m.id === messageId)
    if (!msg) return
    const plans: Record<number, PlanState> = {}
    for (const [k, v] of Object.entries(planStates.value)) {
      if (k.startsWith(`${messageId}#`)) plans[Number(k.slice(messageId.length + 1))] = v
    }
    const meta = JSON.stringify({ planStates: plans })
    msg.meta = meta
    void agentChatService.chatMessageSetMeta(chatId, messageId, meta).catch(() => {})
  }

  /** 某会话当前进行中的计划（running / paused_failed；多个时取首个匹配） */
  function activePlanOf(chatId: string): { key: string; plan: PlanState } | null {
    for (const [key, plan] of Object.entries(planStates.value)) {
      if (plan.status !== 'running' && plan.status !== 'paused_failed') continue
      const messageId = key.slice(0, key.lastIndexOf('#'))
      if (findChatIdOf(messageId) === chatId) return { key, plan }
    }
    return null
  }

  /**
   * 计划推进：计划运行期间每个 run 块完成后调用——
   * 成功标当前步 done 并推进；失败标 failed 并暂停链（paused_failed，等用户重试/跳过/终止）
   */
  function advancePlan(chatId: string, ok: boolean, command: string) {
    const active = activePlanOf(chatId)
    if (!active || active.plan.status !== 'running') return
    // 深拷贝当前计划，避免原地改引用类型漏掉响应式
    const plan: PlanState = JSON.parse(JSON.stringify(active.plan))
    const step = plan.steps[plan.currentIndex]
    if (!step) return
    step.command = command
    if (ok) {
      step.status = 'done'
      plan.currentIndex++
      if (plan.steps.every((s) => s.status === 'done' || s.status === 'skipped')) {
        plan.status = 'done'
      }
    } else {
      step.status = 'failed'
      plan.status = 'paused_failed'
      error.value = `计划第 ${plan.currentIndex + 1} 步执行失败，已暂停（可重试 / 跳过 / 终止计划）`
    }
    setPlanState(active.key, plan)
  }

  /**
   * 确认计划：解析 plan 块编号行为结构化步骤，置 running，
   * 把计划原文回传给 LLM 开始逐步执行（回传原文而非依赖模型回忆上一轮；执行链路计数重置）
   */
  async function confirmPlan(messageId: string, index: number, planText: string) {
    const chatId = findChatIdOf(messageId) ?? activeChatId.value
    if (!chatId || generatingChatId.value) return
    const key = `${messageId}#${index}`
    const existing = planStates.value[key]
    if (existing && existing.status !== 'pending_confirm') return
    setPlanState(key, { steps: parsePlanSteps(planText), currentIndex: 0, status: 'running' })
    autoChain = 0
    await llmSend(chatId, `[计划已确认] 请按以下计划逐步执行：\n${planText}`)
  }

  /** 取消计划（待确认阶段）：纯本地状态，不触发新一轮生成 */
  function cancelPlan(messageId: string, index: number, planText: string) {
    const key = `${messageId}#${index}`
    const existing = planStates.value[key]
    if (existing && existing.status !== 'pending_confirm') return
    setPlanState(key, { steps: parsePlanSteps(planText), currentIndex: 0, status: 'cancelled' })
  }

  /** 消息内是否含未确认的 plan 块（计划模式兜底：计划未确认前不自动执行其中的 run 块） */
  function hasUnconfirmedPlan(message: ChatMessage): boolean {
    const plans = [...stripThink(message.content).matchAll(PLAN_RE)]
    return plans.some((_, i) => {
      const st = planStates.value[`${message.id}#${i}`]
      return !st || st.status === 'pending_confirm'
    })
  }

  /** 重试失败步：已记录命令则直接重新过闸执行；否则通知 LLM 重新输出该步命令 */
  async function retryPlanStep(messageId: string, index: number) {
    const key = `${messageId}#${index}`
    const plan = planStates.value[key]
    if (!plan || plan.status !== 'paused_failed') return
    const chatId = findChatIdOf(messageId)
    if (!chatId || generatingChatId.value) return
    const step = plan.steps[plan.currentIndex]
    if (!step || step.status !== 'failed') return
    const next: PlanState = JSON.parse(JSON.stringify(plan))
    next.status = 'running'
    next.steps[next.currentIndex].status = 'running'
    setPlanState(key, next)
    autoChain = 0
    if (step.command) {
      // 重试用独立的 run key（不复用原 run 块的 done/error 状态守卫）
      await runGated(chatId, `${messageId}#retry${index}-${plan.currentIndex}`, step.command, 'plan', false)
      return
    }
    await llmSend(chatId, `[计划步骤重试] 请重新执行第 ${plan.currentIndex + 1} 步：${step.text}`)
  }

  /** 跳过失败步：标 skipped 继续后续步骤 */
  async function skipPlanStep(messageId: string, index: number) {
    const key = `${messageId}#${index}`
    const plan = planStates.value[key]
    if (!plan || plan.status !== 'paused_failed') return
    const chatId = findChatIdOf(messageId)
    if (!chatId || generatingChatId.value) return
    const step = plan.steps[plan.currentIndex]
    if (!step || step.status !== 'failed') return
    const next: PlanState = JSON.parse(JSON.stringify(plan))
    next.steps[next.currentIndex].status = 'skipped'
    next.currentIndex++
    next.status = next.steps.every((s) => s.status === 'done' || s.status === 'skipped')
      ? 'done'
      : 'running'
    setPlanState(key, next)
    autoChain = 0
    await llmSend(
      chatId,
      `[计划步骤已跳过] 第 ${plan.currentIndex + 1} 步「${step.text}」已跳过，请继续执行后续步骤。`,
    )
  }

  /** 终止计划：状态 cancelled 并通知 LLM */
  async function terminatePlan(messageId: string, index: number) {
    const key = `${messageId}#${index}`
    const plan = planStates.value[key]
    if (!plan || (plan.status !== 'running' && plan.status !== 'paused_failed')) return
    const chatId = findChatIdOf(messageId)
    if (!chatId || generatingChatId.value) return
    setPlanState(key, { ...plan, status: 'cancelled' })
    await llmSend(chatId, '[计划已终止] 用户终止了执行计划，请停止后续命令并总结当前进展。')
  }

  /** 自动链路：执行回复中第一条未执行的 run 命令（查询直接执行；修改类自动模式带批准、否则闸门弹窗确认；危险跳过留给手动） */
  async function autoStep(chatId: string, message: ChatMessage) {
    if (autoChain >= MAX_AUTO_CHAIN) {
      error.value = `自动执行已达上限（${MAX_AUTO_CHAIN} 条），剩余命令请手动执行`
      return
    }
    // 计划模式兜底：计划阶段不应有 run 块；若有且计划未确认，不自动执行
    if (planMode.value && hasUnconfirmedPlan(message)) return
    // 计划失败暂停 / 已终止：停止自动链，等用户在计划卡片上操作
    const activePlan = activePlanOf(chatId)
    if (activePlan && activePlan.plan.status !== 'running') return
    // 思考段内的内容不参与命令解析
    const cmds = [...stripThink(message.content).matchAll(RUN_RE)].map((m) => m[1].trim())
    for (let i = 0; i < cmds.length; i++) {
      if (runStates.value[`${message.id}#${i}`]) continue
      const level = await ensureRisk(cmds[i])
      if (level === 'danger') continue
      autoChain++
      await executeRun(message.id, i, cmds[i], {
        // 自动模式 = 用户对修改类命令的常驻批准（闸门按 approved 放行并记 approved 审计）
        approved: level === 'modify' && autoRun.value,
        source: activePlan ? 'plan' : 'auto',
      })
      // 执行结果触发新一轮生成，其 ai_done 会继续 autoStep
      return
    }
  }

  /** 中断当前生成（本地立即定稿已流式到达的内容） */
  async function abort() {
    const chatId = generatingChatId.value
    if (pendingConfirm.value) resolvePendingConfirm({ ok: false })
    await aiService.chatAbort().catch(() => {})
    generatingChatId.value = null
    if (!chatId) return
    streamBytes.delete(chatId)
    streamLimited.delete(chatId)
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
    currentKey,
    messages,
    busy,
    loading,
    error,
    includeContext,
    sshSessionId,
    autoRun,
    planMode,
    runStates,
    planStates,
    pendingConfirm,
    riskOf,
    ensureRisk,
    resolveSessionForChat,
    execBlockReason,
    resolvePendingConfirm,
    confirmPlan,
    cancelPlan,
    retryPlanStep,
    skipPlanStep,
    terminatePlan,
    newChat,
    switchChat,
    deleteChat,
    clearMessages,
    send,
    executeRun,
    abort,
  }
})
