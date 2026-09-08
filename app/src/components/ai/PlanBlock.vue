<script setup lang="ts">
/**
 * PlanBlock - 计划模式下 AI 回复中的执行计划卡片
 *
 * AI 按计划模式协议输出 ```plan 块，此处渲染为计划卡片：
 * - 待确认：「确认执行」→ 回传计划原文开始逐步执行；「取消」→ 纯本地标记
 * - 已确认/已取消：按钮区替换为状态文案
 * 确认状态（agent store planStates）；确认后的执行仍走 run 块的三级策略
 */
import { computed } from 'vue'
import { NButton, NIcon } from 'naive-ui'
import { Check, X } from '@vicons/tabler'
import { useAgentStore } from '@/stores/agent'

const props = defineProps<{
  messageId: string
  /** plan 块在消息内的序号（与 store planStates key 对应） */
  index: number
  /** 计划原文 */
  content: string
  /** 消息生成中不可操作 */
  disabled?: boolean
}>()

const agent = useAgentStore()

const key = computed(() => `${props.messageId}#${props.index}`)
const state = computed(() => agent.planStates[key.value] ?? 'pending')
</script>

<template>
  <div class="plan-block" :class="[state]">
    <div class="plan-head">
      <span class="plan-title">📋 执行计划</span>
      <span v-if="state === 'confirmed'" class="plan-state confirmed">已确认，按计划执行中</span>
      <span v-else-if="state === 'cancelled'" class="plan-state cancelled">已取消</span>
    </div>
    <div class="plan-body">{{ content }}</div>
    <div v-if="state === 'pending'" class="plan-actions">
      <NButton
        size="tiny"
        type="primary"
        secondary
        :disabled="disabled || agent.busy"
        @click="agent.confirmPlan(messageId, index, content)"
      >
        <template #icon><NIcon :component="Check" :size="12" /></template>
        确认执行
      </NButton>
      <NButton
        size="tiny"
        quaternary
        :disabled="disabled"
        @click="agent.cancelPlan(messageId, index)"
      >
        <template #icon><NIcon :component="X" :size="12" /></template>
        取消
      </NButton>
    </div>
  </div>
</template>

<style scoped>
.plan-block {
  margin: 8px 0;
  border: 1px solid var(--border-color);
  border-left: 3px solid var(--primary);
  border-radius: var(--radius-sm);
  background: var(--bg-elevated);
  padding: 8px 10px;
  overflow: hidden;
}
.plan-block.cancelled {
  opacity: 0.6;
}
.plan-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  margin-bottom: 6px;
}
.plan-title {
  font-size: 11px;
  color: var(--text-tertiary);
  letter-spacing: 0.5px;
}
.plan-state {
  font-size: 11px;
}
.plan-state.confirmed {
  color: var(--success);
}
.plan-state.cancelled {
  color: var(--text-tertiary);
}
.plan-body {
  font-size: 12.5px;
  line-height: 1.7;
  color: var(--text-primary);
  white-space: pre-wrap;
  word-break: break-word;
}
.plan-actions {
  display: flex;
  gap: 8px;
  margin-top: 8px;
}
</style>
