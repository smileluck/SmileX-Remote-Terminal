<script setup lang="ts">
/**
 * GpuTab - GPU 详情
 *
 * 经 SSH 执行 nvidia-smi 采集；每 GPU 一卡：利用率曲线 + 显存进度条 + 温度。
 * 远端无 nvidia-smi 时提示未检测到。
 */
import { computed } from 'vue'
import { useMonitorStore } from '@/stores/monitor'
import { fmtBytes } from '@/utils/format'
import Sparkline from '../Sparkline.vue'

const monitor = useMonitorStore()

const gpus = computed(() => monitor.latest?.gpus ?? [])

/** 按 GPU 序号派生利用率曲线（样本中可能短暂缺该卡，缺省 0） */
function utilSeries(index: number): number[] {
  return monitor.activeSamples.map(
    (s) => s.gpus.find((g) => g.index === index)?.util_percent ?? 0,
  )
}

const gpuViews = computed(() =>
  gpus.value.map((g) => ({
    ...g,
    util: utilSeries(g.index),
    memPercent: g.mem_total_mb > 0 ? (g.mem_used_mb / g.mem_total_mb) * 100 : 0,
    memUsed: fmtBytes(g.mem_used_mb * 1024 * 1024),
    memTotal: fmtBytes(g.mem_total_mb * 1024 * 1024),
  })),
)
</script>

<template>
  <div class="gpu-tab">
    <div v-if="gpuViews.length" class="gpu-list">
      <div v-for="g in gpuViews" :key="g.index" class="gpu-card">
        <div class="gpu-head">
          <span class="gpu-name" :title="g.name">{{ g.name }}</span>
          <span v-if="g.temp_c != null" class="gpu-temp">{{ g.temp_c.toFixed(0) }}°C</span>
        </div>
        <div class="gpu-util-row">
          <span class="gpu-label">利用率</span>
          <span class="gpu-util">{{ g.util_percent.toFixed(0) }}%</span>
        </div>
        <Sparkline :data="g.util" color="#a78bfa" percent :height="40" />
        <div class="gpu-util-row">
          <span class="gpu-label">显存</span>
          <span class="gpu-mem">{{ g.memUsed }} / {{ g.memTotal }}</span>
        </div>
        <div class="mem-bar">
          <div
            class="mem-fill"
            :class="{ warn: g.memPercent >= 85 }"
            :style="{ width: `${Math.min(100, g.memPercent)}%` }"
          />
        </div>
      </div>
    </div>
    <p v-else class="empty">未检测到 NVIDIA GPU（需远端安装 nvidia-smi）</p>
  </div>
</template>

<style scoped>
.gpu-tab {
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.gpu-list {
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.gpu-card {
  background: var(--bg-elevated, #161c24);
  border: 1px solid var(--border-color);
  border-radius: var(--radius-md, 8px);
  padding: 10px 12px;
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.gpu-head {
  display: flex;
  justify-content: space-between;
  align-items: baseline;
  gap: 8px;
}
.gpu-name {
  font-size: 12px;
  font-weight: 500;
  color: var(--text-primary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.gpu-temp {
  font-size: 11px;
  color: var(--text-tertiary);
  font-variant-numeric: tabular-nums;
  flex-shrink: 0;
}
.gpu-util-row {
  display: flex;
  justify-content: space-between;
  align-items: baseline;
}
.gpu-label {
  font-size: 11px;
  color: var(--text-secondary);
}
.gpu-util {
  font-size: 15px;
  font-weight: 600;
  color: #a78bfa;
  font-variant-numeric: tabular-nums;
}
.gpu-mem {
  font-size: 11px;
  color: var(--text-tertiary);
  font-family: var(--font-mono, ui-monospace, monospace);
}
.mem-bar {
  height: 6px;
  background: var(--bg-panel, #10141b);
  border-radius: 3px;
  overflow: hidden;
}
.mem-fill {
  height: 100%;
  background: #a78bfa;
  border-radius: 3px;
  transition: width 0.3s;
}
.mem-fill.warn {
  background: var(--danger, #f87171);
}
.empty {
  font-size: 12px;
  color: var(--text-tertiary);
  text-align: center;
  padding: 24px 0;
}
</style>
