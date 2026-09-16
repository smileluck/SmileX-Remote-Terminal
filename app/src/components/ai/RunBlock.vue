<script setup lang="ts">
/**
 * RunBlock - AI 回复中的可执行命令卡片
 *
 * AI 按命令执行协议输出 ```run 块，此处渲染为命令卡片：
 * - 风险标签经 ai_classify_command 异步查询（先显示「识别中」，回来后响应式更新），仅作展示
 * - 执行决策全部走后端安全闸门（ai_exec_prepare）：闸门拒绝直接展示原因；
 *   修改类未授权时由 store 挂起弹窗（可勾选「记住授权」写白名单）；
 *   危险命令保留此处的二次确认交互，确认后以 approved=true 过闸
 * 命令写入该聊天绑定的终端窗口会话执行（用户可在终端看到全过程），
 * 回显自动捕获为输出；绑定会话断开时阻止执行并提示。
 * 执行状态（agent store runStates）；输出不回显在面板，仅作为 tool 消息
 * 保留在对话中供 LLM 续问上下文使用
 */
import { computed } from 'vue'
import { NButton, NIcon, useDialog, useMessage } from 'naive-ui'
import { Copy, PlayerPlay, QuestionMark } from '@vicons/tabler'
import { useAgentStore } from '@/stores/agent'

const props = defineProps<{
  messageId: string
  /** run 块在消息内的序号（与 store runStates key 对应） */
  index: number
  command: string
  /** 消息生成中不可执行 */
  disabled?: boolean
}>()

const agent = useAgentStore()
const dialog = useDialog()
const message = useMessage()

const key = computed(() => `${props.messageId}#${props.index}`)
const state = computed(() => agent.runStates[key.value])
/** 即时风险标签（异步查询；null = 识别中） */
const level = computed(() => agent.riskOf(props.command))
const danger = computed(() => level.value === 'danger')
const levelLabel = computed(() =>
  level.value ? ({ query: '查询', modify: '修改', danger: '危险' })[level.value] : '识别中…',
)

async function copyCommand() {
  try {
    await navigator.clipboard.writeText(props.command)
    message.success('命令已复制')
  } catch {
    message.error('复制失败')
  }
}

/** 解释命令：作为用户消息走正常聊天流式回复（生成中不打扰，遵循 store 单生成惯例） */
const EXPLAIN_CMD_MAX = 4 * 1024
function explain() {
  if (agent.busy) {
    message.warning('AI 正在生成中，请稍后重试')
    return
  }
  const cmd =
    props.command.length > EXPLAIN_CMD_MAX
      ? `${props.command.slice(0, EXPLAIN_CMD_MAX)}\n…（命令过长，已截断）`
      : props.command
  void agent.send(`请解释下面这条命令的作用、参数含义和潜在风险：\n\`\`\`bash\n${cmd}\n\`\`\``)
}

async function run() {
  const reason = agent.execBlockReason(props.messageId)
  if (reason) {
    message.warning(reason)
    return
  }
  // 等风险识别完成再决定交互（危险命令需二次确认）
  const lvl = level.value ?? (await agent.ensureRisk(props.command))
  if (lvl === 'danger') {
    dialog.warning({
      title: '危险命令确认',
      content: `该命令可能造成破坏性影响：\n\n$ ${props.command}\n\n确定要在服务器上执行吗？`,
      positiveText: '仍要执行',
      negativeText: '取消',
      onPositiveClick: () => {
        void agent.executeRun(props.messageId, props.index, props.command, { approved: true })
      },
    })
    return
  }
  // 查询/修改类：直接执行，闸门未授权时由 store 弹确认框（含「记住授权」）
  await agent.executeRun(props.messageId, props.index, props.command)
}
</script>

<template>
  <div class="run-block" :class="[level ?? '']">
    <div class="run-head">
      <span class="run-title">$ 终端命令 <span class="run-level" :class="[level ?? '']">{{ levelLabel }}</span></span>
      <div class="run-actions">
        <NButton size="tiny" quaternary title="复制命令" @click="copyCommand">
          <template #icon><NIcon :component="Copy" :size="12" /></template>
        </NButton>
        <NButton size="tiny" quaternary title="解释命令" :disabled="disabled" @click="explain">
          <template #icon><NIcon :component="QuestionMark" :size="12" /></template>
        </NButton>
        <NButton
          size="tiny"
          :type="state?.status === 'error' || state?.status === 'rejected' ? 'error' : 'primary'"
          :loading="state?.status === 'running'"
          :disabled="disabled || state?.status === 'done' || state?.status === 'running'"
          secondary
          @click="run"
        >
          <template v-if="state?.status !== 'done' && state?.status !== 'running'" #icon>
            <NIcon :component="PlayerPlay" :size="12" />
          </template>
          {{ state?.status === 'done' ? '已执行' : state?.status === 'error' ? '重试' : '执行' }}
        </NButton>
      </div>
    </div>
    <code class="run-cmd">{{ command }}</code>
    <div v-if="state?.status === 'rejected'" class="run-warn rejected">✕ {{ state.output }}</div>
    <div v-else-if="danger" class="run-warn">⚠ 高风险命令，执行前请确认影响</div>
    <div v-else-if="level === 'modify'" class="run-warn modify">⚠ 修改类命令，可能变更服务器状态</div>
  </div>
</template>

<style scoped>
.run-block {
  margin: 8px 0;
  border: 1px solid var(--border-color);
  border-left: 3px solid var(--primary);
  border-radius: var(--radius-sm);
  background: var(--bg-elevated);
  padding: 8px 10px;
  overflow: hidden;
}
.run-block.danger {
  border-left-color: var(--danger);
}
.run-block.modify {
  border-left-color: var(--warning);
}
.run-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  margin-bottom: 6px;
}
.run-actions {
  display: flex;
  align-items: center;
  gap: 4px;
  flex-shrink: 0;
}
.run-title {
  font-size: 11px;
  color: var(--text-tertiary);
  letter-spacing: 0.5px;
}
.run-cmd {
  display: block;
  font-family: ui-monospace, Consolas, 'Courier New', monospace;
  font-size: 12.5px;
  color: var(--text-primary);
  white-space: pre-wrap;
  word-break: break-all;
}
.run-warn {
  margin-top: 6px;
  font-size: 11px;
  color: var(--warning);
}
.run-warn.modify {
  color: var(--warning);
  opacity: 0.85;
}
.run-warn.rejected {
  color: var(--danger);
}
.run-level {
  display: inline-block;
  margin-left: 6px;
  padding: 0 5px;
  border-radius: 3px;
  font-size: 10px;
  letter-spacing: 0;
}
.run-level.query {
  color: var(--success);
  background: color-mix(in srgb, var(--success) 12%, transparent);
}
.run-level.modify {
  color: var(--warning);
  background: color-mix(in srgb, var(--warning) 12%, transparent);
}
.run-level.danger {
  color: var(--danger);
  background: color-mix(in srgb, var(--danger) 12%, transparent);
}
</style>
