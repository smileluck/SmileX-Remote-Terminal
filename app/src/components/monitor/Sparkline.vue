<script setup lang="ts">
/**
 * Sparkline - 轻量 SVG 折线图
 *
 * 自绘实现（不引入图表库），用于监控指标的实时曲线。
 */
import { computed } from 'vue'

const props = withDefaults(
  defineProps<{
    /** 数据点（旧 → 新） */
    data: number[]
    /** 0–100 百分比数据时自动填充面积 */
    percent?: boolean
    /** 线色 */
    color?: string
    /** 高度（px） */
    height?: number
  }>(),
  { color: 'var(--primary)', height: 44, percent: false },
)

const W = 100 // viewBox 宽度（等比缩放）

const path = computed(() => {
  const d = props.data
  if (d.length < 2) return { line: '', area: '' }
  const max = props.percent ? 100 : Math.max(...d) * 1.1 || 1
  const min = props.percent ? 0 : Math.min(...d)
  const range = max - min || 1
  const step = W / (d.length - 1)
  const pts = d.map((v, i) => {
    const x = i * step
    const y = 100 - ((v - min) / range) * 100
    return `${x.toFixed(1)},${y.toFixed(1)}`
  })
  const line = `M${pts.join(' L')}`
  const area = `${line} L${W},100 L0,100 Z`
  return { line, area }
})
</script>

<template>
  <svg
    class="sparkline"
    :viewBox="`0 0 ${W} 100`"
    preserveAspectRatio="none"
    :style="{ height: `${height}px` }"
  >
    <path v-if="path.area" :d="path.area" :fill="color" opacity="0.12" />
    <path
      v-if="path.line"
      :d="path.line"
      fill="none"
      :stroke="color"
      stroke-width="2"
      vector-effect="non-scaling-stroke"
      stroke-linejoin="round"
      stroke-linecap="round"
    />
  </svg>
</template>

<style scoped>
.sparkline {
  display: block;
  width: 100%;
}
</style>
