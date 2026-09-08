<script setup lang="ts">
/**
 * RunBlock - AI 回复中的可执行命令卡片
 *
 * AI 按命令执行协议输出 ```run 块，此处渲染为命令卡片，按三级分类处理：
 * - 查询（只读）：点击直接执行；自动链路中始终自动执行
 * - 修改：点击弹确认框；自动链路中仅「自动执行」开启时直接执行，否则逐条弹窗确认
 * - 危险：永不自动执行，手动执行需二次确认
 * 命令写入该聊天绑定的终端窗口会话执行（用户可在终端看到全过程），
 * 回显自动捕获为输出；绑定会话断开时阻止执行并提示。
 * 执行状态（agent store runStates）；输出不回显在面板，仅作为 tool 消息
 * 保留在对话中供 LLM 续问上下文使用
 */
import { computed } from 'vue'
import { NButton, NIcon, useDialog, useMessage } from 'naive-ui'
import { Copy, PlayerPlay } from '@vicons/tabler'
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
const level = computed(() => agent.classifyCommand(props.command))
const danger = computed(() => level.value === 'danger')
const levelLabel = computed(
  () => ({ query: '查询', modify: '修改', danger: '危险' })[level.value],
)

async function copyCommand() {
  try {
    await navigator.clipboard.writeText(props.command)
    message.success('命令已复制')
  } catch {
    message.error('复制失败')
  }
}

async function run() {
  const reason = agent.execBlockReason(props.messageId)
  if (reason) {
    message.warning(reason)
    return
  }
  if (danger.value) {
    dialog.warning({
      title: '危险命令确认',
      content: `该命令可能造成破坏性影响：\n\n$ ${props.command}\n\n确定要在服务器上执行吗？`,
      positiveText: '仍要执行',
      negativeText: '取消',
      onPositiveClick: () => {
        void agent.executeRun(props.messageId, props.index, props.command)
      },
    })
    return
  }
  if (level.value === 'modify') {
    dialog.warning({
      title: '修改类命令确认',
      content: `该命令可能修改服务器状态：\n\n$ ${props.command}\n\n确定执行吗？`,
      positiveText: '执行',
      negativeText: '取消',
      onPositiveClick: () => {
        void agent.executeRun(props.messageId, props.index, props.command)
      },
    })
    return
  }
  await agent.executeRun(props.messageId, props.index, props.command)
}
</script>

<template>
  <div class="run-block" :class="[level]">
    <div class="run-head">
      <span class="run-title">$ 终端命令 <span class="run-level" :class="[level]">{{ levelLabel }}</span></span>
      <div class="run-actions">
        <NButton size="tiny" quaternary title="复制命令" @click="copyCommand">
          <template #icon><NIcon :component="Copy" :size="12" /></template>
        </NButton>
        <NButton
          size="tiny"
          :type="state?.status === 'error' ? 'error' : 'primary'"
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
    <div v-if="danger" class="run-warn">⚠ 高风险命令，执行前请确认影响</div>
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
