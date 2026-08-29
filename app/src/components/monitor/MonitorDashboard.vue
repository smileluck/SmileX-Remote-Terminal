<script setup lang="ts">
/**
 * MonitorDashboard - 监控看版内容区
 *
 * - 顶部：监控目标切换（多会话下拉；默认跟随激活 SSH tab）
 * - 指标卡片：CPU / 内存 / 网络 / 磁盘（Sparkline 实时曲线）
 * - 底部：会话健康列表
 *
 * 数据来源：monitor store（`monitor_metrics` 事件 + session_stats_all 轮询）
 */
import { computed, watch, onMounted } from 'vue'
import { NSelect, NTag } from 'naive-ui'
import { useMonitorStore } from '@/stores/monitor'
import { useTabsStore } from '@/stores/tabs'
import MetricCard from './MetricCard.vue'
import SessionHealthList from './SessionHealthList.vue'

const monitor = useMonitorStore()
const tabs = useTabsStore()

/** 监控目标下拉选项（SSH 会话） */
const targetOptions = computed(() =>
  tabs.tabs
    .filter((t) => t.kind === 'ssh' && t.sessionId)
    .map((t) => ({ label: t.title, value: t.sessionId! })),
)

/** 跟随激活的 SSH tab 自动切换监控目标 */
watch(
  () => tabs.activeTab,
  (tab) => {
    if (tab?.kind === 'ssh' && tab.sessionId) {
      monitor.setActive(tab.sessionId)
    }
  },
  { immediate: true },
)

onMounted(() => monitor.init())

/** 派生曲线数据 */
const cpuData = computed(() =>
  monitor.activeSamples.map((s) => s.cpu_percent ?? 0),
)
const memData = computed(() => monitor.activeSamples.map((s) => s.mem_percent))
const rxData = computed(() => monitor.activeSamples.map((s) => s.net_rx_bps))
const txData = computed(() => monitor.activeSamples.map((s) => s.net_tx_bps))

const latest = computed(() => monitor.latest)

function fmtBytes(n: number): string {
  if (n < 1024) return `${n.toFixed(0)} B`
  if (n < 1024 ** 2) return `${(n / 1024).toFixed(1)} KB`
  if (n < 1024 ** 3) return `${(n / 1024 ** 2).toFixed(1)} MB`
  return `${(n / 1024 ** 3).toFixed(2)} GB`
}

function fmtBps(n: number): string {
  return `${fmtBytes(n)}/s`
}

function fmtUptime(s: number): string {
  if (!s) return '-'
  const d = Math.floor(s / 86400)
  const h = Math.floor((s % 86400) / 3600)
  if (d > 0) return `${d} 天 ${h} 小时`
  const m = Math.floor((s % 3600) / 60)
  if (h > 0) return `${h} 小时 ${m} 分`
  return `${m} 分`
}

const rootDisk = computed(() => latest.value?.disks.find((d) => d.mount === '/'))

/** CPU 颜色分级 */
const cpuColor = computed(() => {
  const v = latest.value?.cpu_percent ?? 0
  if (v >= 90) return 'var(--danger, #f87171)'
  if (v >= 70) return 'var(--warning, #fbbf24)'
  return 'var(--success, #34d399)'
})
</script>

<template>
  <div class="monitor-dashboard">
    <!-- 监控目标切换 -->
    <div v-if="targetOptions.length > 0" class="target-row">
      <NSelect
        size="small"
        :value="monitor.activeSessionId"
        :options="targetOptions"
        placeholder="选择监控目标"
        @update:value="(v: string) => monitor.setActive(v)"
      />
    </div>

    <template v-if="latest">
      <!-- 概览行：延迟 / 负载 / 运行时长 -->
      <div class="overview-row">
        <NTag v-if="latest.latency_ms != null" size="small" :bordered="false">
          延迟 {{ latest.latency_ms }} ms
        </NTag>
        <NTag size="small" :bordered="false">负载 {{ latest.load1.toFixed(2) }}</NTag>
        <NTag size="small" :bordered="false">运行 {{ fmtUptime(latest.uptime_s) }}</NTag>
      </div>

      <MetricCard
        title="CPU"
        :value="latest.cpu_percent != null ? `${latest.cpu_percent.toFixed(1)}%` : '…'"
        :data="cpuData"
        :color="cpuColor"
        percent
      />
      <MetricCard
        title="内存"
        :value="`${latest.mem_percent.toFixed(1)}%`"
        :sub="`${fmtBytes(latest.mem_used_bytes)} / ${fmtBytes(latest.mem_total_bytes)}`"
        :data="memData"
        color="#3a7afe"
        percent
      />
      <MetricCard
        title="网络 ↓ 下行"
        :value="fmtBps(latest.net_rx_bps)"
        :data="rxData"
        color="#34d399"
      />
      <MetricCard
        title="网络 ↑ 上行"
        :value="fmtBps(latest.net_tx_bps)"
        :data="txData"
        color="#fbbf24"
      />

      <!-- 磁盘列表 -->
      <div v-if="latest.disks.length" class="disk-list">
        <div class="section-title">磁盘</div>
        <div v-for="d in latest.disks.slice(0, 6)" :key="d.mount" class="disk-row">
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
      <p v-if="rootDisk" class="disk-hint">
        / 共 {{ (rootDisk.total_kb / 1024 / 1024).toFixed(1) }} GB
      </p>
    </template>

    <p v-else class="placeholder">
      {{ targetOptions.length ? '正在采集监控数据…' : '连接 SSH 会话后展示监控数据' }}
    </p>

    <SessionHealthList />
  </div>
</template>

<style scoped>
.monitor-dashboard {
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.target-row {
  display: flex;
}
.overview-row {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}
.placeholder {
  font-size: 12px;
  color: var(--text-tertiary);
  text-align: center;
  padding: 24px 0;
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
.disk-hint {
  font-size: 11px;
  color: var(--text-tertiary);
}
</style>
