<script setup lang="ts">
/**
 * MetricCard - 指标卡片（标题 + 当前值 + 单位 + Sparkline 曲线）
 */
import Sparkline from './Sparkline.vue'

withDefaults(
  defineProps<{
    title: string
    /** 当前值展示文本 */
    value: string
    /** 辅助信息（如副标题/明细） */
    sub?: string
    data: number[]
    color?: string
    percent?: boolean
  }>(),
  { color: 'var(--primary)', percent: false, sub: '' },
)
</script>

<template>
  <div class="metric-card">
    <div class="card-head">
      <span class="card-title">{{ title }}</span>
      <span class="card-value" :style="{ color }">{{ value }}</span>
    </div>
    <Sparkline :data="data" :color="color" :percent="percent" :height="40" />
    <div v-if="sub" class="card-sub">{{ sub }}</div>
  </div>
</template>

<style scoped>
.metric-card {
  background: var(--bg-elevated, #161c24);
  border: 1px solid var(--border-color);
  border-radius: var(--radius-md, 8px);
  padding: 10px 12px;
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.card-head {
  display: flex;
  justify-content: space-between;
  align-items: baseline;
}
.card-title {
  font-size: 11px;
  color: var(--text-secondary);
  text-transform: uppercase;
  letter-spacing: 0.5px;
}
.card-value {
  font-size: 15px;
  font-weight: 600;
  font-variant-numeric: tabular-nums;
}
.card-sub {
  font-size: 11px;
  color: var(--text-tertiary);
  font-family: var(--font-mono, ui-monospace, monospace);
}
</style>
