<script setup lang="ts">
/**
 * RunBlock - AI 回复中的可执行命令卡片
 *
 * AI 按命令执行协议输出 ```run 块，此处渲染为命令卡片：
 * - 手动点击「执行」→ 危险命令先二次确认 → session_exec 在绑定的服务器执行
 * - 自动执行模式下由 agent store 直接触发，危险命令仍跳过留给手动确认
 * - 执行状态与输出回显（agent store runStates）
 */
import { computed } from 'vue'
import { NButton, NIcon, useDialog, useMessage } from 'naive-ui'
import { PlayerPlay } from '@vicons/tabler'
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
const danger = computed(() => agent.isDangerous(props.command))

async function run() {
  if (!agent.sshSessionId) {
    message.warning('未连接 SSH 服务器，请先连接')
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
  await agent.executeRun(props.messageId, props.index, props.command)
}
</script>

<template>
  <div class="run-block" :class="{ danger }">
    <div class="run-head">
      <span class="run-title">$ 终端命令</span>
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
    <code class="run-cmd">{{ command }}</code>
    <div v-if="danger" class="run-warn">⚠ 高风险命令，执行前请确认影响</div>
    <pre v-if="state?.output" class="run-out">{{ state.output }}</pre>
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
.run-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  margin-bottom: 6px;
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
.run-out {
  margin: 8px 0 0;
  padding: 8px 10px;
  background: #0d1117;
  border-radius: var(--radius-sm);
  color: var(--text-secondary);
  font-family: ui-monospace, Consolas, 'Courier New', monospace;
  font-size: 12px;
  white-space: pre-wrap;
  word-break: break-all;
  max-height: 200px;
  overflow-y: auto;
}
</style>
