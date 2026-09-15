<script setup lang="ts">
/**
 * PlanBlock - 计划模式下 AI 回复中的执行计划卡片
 *
 * AI 按计划模式协议输出 ```plan 块，此处渲染为结构化步骤状态机：
 * - pending_confirm：步骤列表预览 +「确认执行 / 取消」
 * - running：高亮当前步；某步失败 → paused_failed（暂停自动链），
 *   失败步旁提供「重试 / 跳过 / 终止计划」
 * - done / cancelled：仅展示最终状态
 * 状态存于 agent store planStates，并持久化到消息 meta（重启可恢复；
 * 恢复时 running 一律按 paused_failed 处理，不自动续跑）
 */
import { computed } from 'vue'
import { NButton, NIcon } from 'naive-ui'
import {
  Check,
  X,
  Circle,
  CircleCheck,
  CircleX,
  PlayerPlay,
  PlayerSkipForward,
  PlayerStop,
  Refresh,
} from '@vicons/tabler'
import { useAgentStore, parsePlanSteps } from '@/stores/agent'
import type { PlanStep, PlanStepStatus } from '@/types/ai'

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
/** 结构化状态；无记录 = 待确认（steps 现场解析用于预览） */
const plan = computed(() => agent.planStates[key.value] ?? null)
const status = computed(() => plan.value?.status ?? 'pending_confirm')
const steps = computed<PlanStep[]>(() => plan.value?.steps ?? parsePlanSteps(props.content))

const STATUS_TEXT: Record<string, string> = {
  running: '执行中',
  paused_failed: '已暂停（步骤失败）',
  done: '计划已完成',
  cancelled: '已取消',
}

/** 步骤状态图标 */
const STEP_ICON: Record<PlanStepStatus, typeof Circle> = {
  pending: Circle,
  running: PlayerPlay,
  done: CircleCheck,
  failed: CircleX,
  skipped: PlayerSkipForward,
}

/** 失败中的当前步（paused_failed 时展示操作按钮） */
const failedStep = computed(() => {
  if (!plan.value || plan.value.status !== 'paused_failed') return -1
  const i = plan.value.currentIndex
  return plan.value.steps[i]?.status === 'failed' ? i : -1
})
</script>

<template>
  <div class="plan-block" :class="[status]">
    <div class="plan-head">
      <span class="plan-title">📋 执行计划</span>
      <span v-if="status !== 'pending_confirm'" class="plan-state" :class="[status]">
        {{ STATUS_TEXT[status] }}
      </span>
    </div>
    <ol class="plan-steps">
      <li
        v-for="(step, i) in steps"
        :key="i"
        class="plan-step"
        :class="[
          step.status,
          { current: plan && plan.status !== 'done' && i === plan.currentIndex },
        ]"
      >
        <NIcon :component="STEP_ICON[step.status]" :size="13" class="step-icon" />
        <span class="step-text">{{ i + 1 }}. {{ step.text }}</span>
        <span v-if="i === failedStep" class="step-actions">
          <NButton
            size="tiny"
            type="primary"
            quaternary
            :disabled="disabled || agent.busy"
            title="重新执行该步命令"
            @click="agent.retryPlanStep(messageId, index)"
          >
            <template #icon><NIcon :component="Refresh" :size="12" /></template>
            重试
          </NButton>
          <NButton
            size="tiny"
            quaternary
            :disabled="disabled || agent.busy"
            title="标记跳过并继续后续步骤"
            @click="agent.skipPlanStep(messageId, index)"
          >
            <template #icon><NIcon :component="PlayerSkipForward" :size="12" /></template>
            跳过
          </NButton>
          <NButton
            size="tiny"
            type="error"
            quaternary
            :disabled="disabled || agent.busy"
            title="终止整个计划"
            @click="agent.terminatePlan(messageId, index)"
          >
            <template #icon><NIcon :component="PlayerStop" :size="12" /></template>
            终止计划
          </NButton>
        </span>
      </li>
    </ol>
    <div v-if="status === 'pending_confirm'" class="plan-actions">
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
        @click="agent.cancelPlan(messageId, index, content)"
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
.plan-block.paused_failed {
  border-left-color: var(--danger);
}
.plan-block.done {
  border-left-color: var(--success);
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
.plan-state.running {
  color: var(--primary);
}
.plan-state.paused_failed {
  color: var(--danger);
}
.plan-state.done {
  color: var(--success);
}
.plan-state.cancelled {
  color: var(--text-tertiary);
}
.plan-steps {
  list-style: none;
  margin: 0;
  padding: 0;
}
.plan-step {
  display: flex;
  align-items: baseline;
  gap: 6px;
  font-size: 12.5px;
  line-height: 1.7;
  color: var(--text-secondary);
  padding: 2px 4px;
  border-radius: var(--radius-sm);
}
.plan-step.current {
  background: var(--primary-bg);
  color: var(--text-primary);
}
.step-icon {
  flex-shrink: 0;
  align-self: center;
  color: var(--text-tertiary);
}
.plan-step.done .step-icon {
  color: var(--success);
}
.plan-step.failed .step-icon {
  color: var(--danger);
}
.plan-step.skipped .step-icon {
  color: var(--warning);
}
.plan-step.running .step-icon {
  color: var(--primary);
}
.plan-step.done .step-text,
.plan-step.skipped .step-text {
  color: var(--text-tertiary);
}
.plan-step.failed .step-text {
  color: var(--danger);
}
.step-text {
  flex: 1;
  min-width: 0;
  white-space: pre-wrap;
  word-break: break-word;
}
.step-actions {
  display: flex;
  align-items: center;
  gap: 2px;
  flex-shrink: 0;
  align-self: center;
}
.plan-actions {
  display: flex;
  gap: 8px;
  margin-top: 8px;
}
</style>
