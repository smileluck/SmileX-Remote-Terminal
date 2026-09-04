<script setup lang="ts">
/**
 * GeneralTab - 通用概览
 *
 * 概览行（延迟/负载/运行时长）+ CPU/内存/网络摘要卡 + 磁盘 Top3 + 会话健康列表。
 * 详细数据见 CPU/内存/磁盘等分类 Tab。
 */
import { computed } from 'vue'
import { NTag } from 'naive-ui'
import { useMonitorStore } from '@/stores/monitor'
import { fmtBytes, fmtBps, fmtUptime } from '@/utils/format'
import MetricCard from '../MetricCard.vue'
import SessionHealthList from '../SessionHealthList.vue'

const monitor = useMonitorStore()

const latest = computed(() => monitor.latest)
const cpuData = computed(() => monitor.activeSamples.map((s) => s.cpu_percent ?? 0))
const memData = computed(() => monitor.activeSamples.map((s) => s.mem_percent))
const rxData = computed(() => monitor.activeSamples.map((s) => s.net_rx_bps))
const txData = computed(() => monitor.activeSamples.map((s) => s.net_tx_bps))

/** CPU 颜色分级 */
const cpuColor = computed(() => {
  const v = latest.value?.cpu_percent ?? 0
  if (v >= 90) return 'var(--danger, #f87171)'
  if (v >= 70) return 'var(--warning, #fbbf24)'
  return 'var(--success, #34d399)'
})

/** 磁盘排序：根分区优先，其余按容量降序 */
const topDisks = computed(() => {
  const disks = [...(latest.value?.disks ?? [])]
  disks.sort((a, b) => {
    if (a.mount === '/') return -1
    if (b.mount === '/') return 1
    return b.total_kb - a.total_kb
  })
  return disks.slice(0, 3)
})
</script>

<template>
  <div class="general-tab">
    <!-- 概览行：延迟 / 负载 / 运行时长 -->
    <div v-if="latest" class="overview-row">
      <NTag v-if="latest.latency_ms != null" size="small" :bordered="false">
        延迟 {{ latest.latency_ms }} ms
      </NTag>
      <NTag size="small" :bordered="false">负载 {{ latest.load1.toFixed(2) }}</NTag>
      <NTag size="small" :bordered="false">运行 {{ fmtUptime(latest.uptime_s) }}</NTag>
    </div>

    <MetricCard
      title="CPU"
      :value="latest?.cpu_percent != null ? `${latest.cpu_percent.toFixed(1)}%` : '…'"
      :data="cpuData"
      :color="cpuColor"
      percent
    />
    <MetricCard
      title="内存"
      :value="latest ? `${latest.mem_percent.toFixed(1)}%` : '…'"
      :sub="latest ? `${fmtBytes(latest.mem_used_bytes)} / ${fmtBytes(latest.mem_total_bytes)}` : ''"
      :data="memData"
      color="#4c8dff"
      percent
    />
    <MetricCard
      title="网络 ↓ 下行"
      :value="latest ? fmtBps(latest.net_rx_bps) : '…'"
      :data="rxData"
      color="#34d399"
    />
    <MetricCard
      title="网络 ↑ 上行"
      :value="latest ? fmtBps(latest.net_tx_bps) : '…'"
      :data="txData"
      color="#fbbf24"
    />

    <!-- 磁盘 Top3（完整列表见磁盘 Tab） -->
    <div v-if="topDisks.length" class="disk-list">
      <div class="section-title">磁盘</div>
      <div v-for="d in topDisks" :key="d.mount" class="disk-row">
        <span class="disk-mount" :title="d.mount">{{ d.mount }}</span>
        <div class="disk-bar">
          <div
            class="disk-fill"
            :class="{ warn: d.used_percent >= 85 }"
            :style="{ width: `${Math.min(100, d.used_percent)}%` }"
          />
        </div>
        <span class="disk-pct">{{ d.used_percent.toFixed(0) }}%</span>
      </div>
    </div>

    <SessionHealthList />
  </div>
</template>

<style scoped>
.general-tab {
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.overview-row {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}
.section-title {
  font-size: 11px;
  font-weight: 600;
  color: var(--text-secondary);
  text-transform: uppercase;
  letter-spacing: 0.5px;
}
.disk-list {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.disk-row {
  display: flex;
  align-items: center;
  gap: 8px;
}
.disk-mount {
  width: 72px;
  font-size: 11px;
  color: var(--text-secondary);
  font-family: var(--font-mono, ui-monospace, monospace);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  flex-shrink: 0;
}
.disk-bar {
  flex: 1;
  height: 6px;
  background: var(--bg-elevated);
  border-radius: 3px;
  overflow: hidden;
}
.disk-fill {
  height: 100%;
  background: var(--primary);
  border-radius: 3px;
  transition: width 0.3s;
}
.disk-fill.warn {
  background: var(--danger, #f87171);
}
.disk-pct {
  width: 36px;
  text-align: right;
  font-size: 11px;
  color: var(--text-tertiary);
  font-variant-numeric: tabular-nums;
  flex-shrink: 0;
}
</style>
