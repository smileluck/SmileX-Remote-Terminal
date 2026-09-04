<script setup lang="ts">
/**
 * CpuTab - CPU 详情
 *
 * CPU 使用率曲线 + 负载曲线（1/5/15 分钟）。
 */
import { computed } from 'vue'
import { useMonitorStore } from '@/stores/monitor'
import MetricCard from '../MetricCard.vue'

const monitor = useMonitorStore()

const latest = computed(() => monitor.latest)
const cpuData = computed(() => monitor.activeSamples.map((s) => s.cpu_percent ?? 0))
const loadData = computed(() => monitor.activeSamples.map((s) => s.load1))

/** CPU 颜色分级 */
const cpuColor = computed(() => {
  const v = latest.value?.cpu_percent ?? 0
  if (v >= 90) return 'var(--danger, #f87171)'
  if (v >= 70) return 'var(--warning, #fbbf24)'
  return 'var(--success, #34d399)'
})
</script>

<template>
  <div class="cpu-tab">
    <MetricCard
      title="CPU 使用率"
      :value="latest?.cpu_percent != null ? `${latest.cpu_percent.toFixed(1)}%` : '…'"
      :data="cpuData"
      :color="cpuColor"
      percent
    />
    <MetricCard
      title="负载 (1 分钟)"
      :value="latest ? latest.load1.toFixed(2) : '…'"
      :data="loadData"
      color="#a78bfa"
    />
    <div v-if="latest" class="load-detail">
      <div class="load-item">
        <span class="load-label">1 分钟</span>
        <span class="load-value">{{ latest.load1.toFixed(2) }}</span>
      </div>
      <div class="load-item">
        <span class="load-label">5 分钟</span>
        <span class="load-value">{{ latest.load5.toFixed(2) }}</span>
      </div>
      <div class="load-item">
        <span class="load-label">15 分钟</span>
        <span class="load-value">{{ latest.load15.toFixed(2) }}</span>
      </div>
    </div>
  </div>
</template>

<style scoped>
.cpu-tab {
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.load-detail {
  background: var(--bg-elevated, #161c24);
  border: 1px solid var(--border-color);
  border-radius: var(--radius-md, 8px);
  padding: 8px 12px;
  display: flex;
  justify-content: space-between;
}
.load-item {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 2px;
}
.load-label {
  font-size: 10px;
  color: var(--text-tertiary);
}
.load-value {
  font-size: 14px;
  font-weight: 600;
  font-family: var(--font-mono, ui-monospace, monospace);
  font-variant-numeric: tabular-nums;
  color: var(--text-primary);
}
</style>
