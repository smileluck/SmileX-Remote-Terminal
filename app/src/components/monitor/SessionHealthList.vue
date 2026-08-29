<script setup lang="ts">
/**
 * SessionHealthList - 会话健康列表
 *
 * 展示各 SSH 会话：状态灯、时长、流量、监控延迟。
 * 点击某项切换右栏监控目标。
 */
import { computed } from 'vue'
import { useMonitorStore } from '@/stores/monitor'
import { useTabsStore } from '@/stores/tabs'

const monitor = useMonitorStore()
const tabs = useTabsStore()

/** sessionId → tab 标题（找不到时用短 id） */
const titleOf = (sid: string) =>
  tabs.tabs.find((t) => t.sessionId === sid)?.title ?? sid.slice(0, 8)

/** 最近采样延迟（ms），无数据显示 - */
function latencyOf(sid: string): string {
  const s = monitor.samples[sid]?.at(-1)
  return s?.latency_ms != null ? `${s.latency_ms} ms` : '-'
}

function fmtBytes(n: number): string {
  if (n < 1024) return `${n} B`
  if (n < 1024 ** 2) return `${(n / 1024).toFixed(1)} KB`
  if (n < 1024 ** 3) return `${(n / 1024 ** 2).toFixed(1)} MB`
  return `${(n / 1024 ** 3).toFixed(2)} GB`
}

function fmtDuration(ms: number): string {
  const s = Math.max(0, Math.floor((Date.now() - ms) / 1000))
  const h = Math.floor(s / 3600)
  const m = Math.floor((s % 3600) / 60)
  if (h > 0) return `${h}h ${m}m`
  if (m > 0) return `${m}m ${s % 60}s`
  return `${s}s`
}

const items = computed(() =>
  monitor.sessionHealths.map((h) => ({
    ...h,
    title: titleOf(h.session_id),
    latency: latencyOf(h.session_id),
    duration: fmtDuration(h.connected_at_ms),
    rx: fmtBytes(h.rx_bytes),
    tx: fmtBytes(h.tx_bytes),
    active: monitor.activeSessionId === h.session_id,
  })),
)
</script>

<template>
  <div class="health-list">
    <div class="section-title">会话健康</div>
    <p v-if="items.length === 0" class="empty">暂无活跃会话</p>
    <div
      v-for="it in items"
      :key="it.session_id"
      class="health-item"
      :class="{ active: it.active }"
      @click="monitor.setActive(it.session_id)"
    >
      <span class="dot" :class="it.connected ? 'on' : 'off'" />
      <div class="item-main">
        <div class="item-title">{{ it.title }}</div>
        <div class="item-meta">
          {{ it.duration }} · ↓{{ it.rx }} ↑{{ it.tx }} · {{ it.latency }}
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.health-list {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.section-title {
  font-size: 11px;
  font-weight: 600;
  color: var(--text-secondary);
  text-transform: uppercase;
  letter-spacing: 0.5px;
  margin-bottom: 2px;
}
.empty {
  font-size: 12px;
  color: var(--text-tertiary);
  padding: 8px 0;
}
.health-item {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 10px;
  border-radius: var(--radius-md, 8px);
  border: 1px solid transparent;
  cursor: pointer;
  transition: background 0.12s;
}
.health-item:hover {
  background: var(--bg-elevated);
}
.health-item.active {
  background: var(--bg-elevated);
  border-color: var(--primary);
}
.dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  flex-shrink: 0;
}
.dot.on {
  background: var(--success, #34d399);
  box-shadow: 0 0 6px var(--success, #34d399);
}
.dot.off {
  background: var(--danger, #f87171);
}
.item-main {
  flex: 1;
  min-width: 0;
}
.item-title {
  font-size: 12px;
  font-weight: 500;
  color: var(--text-primary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.item-meta {
  font-size: 11px;
  color: var(--text-tertiary);
  font-family: var(--font-mono, ui-monospace, monospace);
  margin-top: 2px;
}
</style>
