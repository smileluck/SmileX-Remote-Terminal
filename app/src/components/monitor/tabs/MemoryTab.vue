<script setup lang="ts">
/**
 * MemoryTab - 内存详情
 *
 * 内存使用率曲线 + Swap 使用（远端未启用 Swap 时提示）。
 */
import { computed } from 'vue'
import { useMonitorStore } from '@/stores/monitor'
import { fmtBytes } from '@/utils/format'
import MetricCard from '../MetricCard.vue'

const monitor = useMonitorStore()

const latest = computed(() => monitor.latest)
const memData = computed(() => monitor.activeSamples.map((s) => s.mem_percent))

/** Swap 使用率序列（未启用时全 0，不展示曲线） */
const swapData = computed(() =>
  monitor.activeSamples.map((s) =>
    s.swap_total_bytes > 0 ? (s.swap_used_bytes / s.swap_total_bytes) * 100 : 0,
  ),
)

const swap = computed(() => {
  const s = latest.value
  if (!s || s.swap_total_bytes === 0) return null
  return {
    percent: (s.swap_used_bytes / s.swap_total_bytes) * 100,
    used: fmtBytes(s.swap_used_bytes),
    total: fmtBytes(s.swap_total_bytes),
  }
})
</script>

<template>
  <div class="memory-tab">
    <MetricCard
      title="内存使用率"
      :value="latest ? `${latest.mem_percent.toFixed(1)}%` : '…'"
      :sub="latest ? `${fmtBytes(latest.mem_used_bytes)} / ${fmtBytes(latest.mem_total_bytes)}` : ''"
      :data="memData"
      color="#4c8dff"
      percent
    />

    <MetricCard
      v-if="swap"
      title="Swap 使用率"
      :value="`${swap.percent.toFixed(1)}%`"
      :sub="`${swap.used} / ${swap.total}`"
      :data="swapData"
      color="#fbbf24"
      percent
    />
    <p v-else class="swap-empty">远端未启用 Swap</p>
  </div>
</template>

<style scoped>
.memory-tab {
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.swap-empty {
  font-size: 12px;
  color: var(--text-tertiary);
  text-align: center;
  padding: 8px 0;
}
</style>
