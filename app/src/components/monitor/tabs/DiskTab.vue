<script setup lang="ts">
/**
 * DiskTab - 磁盘详情
 *
 * 全部挂载点使用情况（根分区优先，其余按容量降序），含容量/已用明细。
 */
import { computed } from 'vue'
import { useMonitorStore } from '@/stores/monitor'
import { fmtBytes } from '@/utils/format'

const monitor = useMonitorStore()

/** 磁盘排序：根分区优先，其余按容量降序 */
const disks = computed(() => {
  const list = [...(monitor.latest?.disks ?? [])]
  list.sort((a, b) => {
    if (a.mount === '/') return -1
    if (b.mount === '/') return 1
    return b.total_kb - a.total_kb
  })
  return list.map((d) => ({
    ...d,
    used: fmtBytes(d.used_kb * 1024),
    total: fmtBytes(d.total_kb * 1024),
  }))
})
</script>

<template>
  <div class="disk-tab">
    <div v-if="disks.length" class="disk-list">
      <div v-for="d in disks" :key="d.mount" class="disk-item">
        <div class="disk-head">
          <span class="disk-mount" :title="d.mount">{{ d.mount }}</span>
          <span class="disk-pct" :class="{ warn: d.used_percent >= 85 }">
            {{ d.used_percent.toFixed(0) }}%
          </span>
        </div>
        <div class="disk-bar">
          <div
            class="disk-fill"
            :class="{ warn: d.used_percent >= 85 }"
            :style="{ width: `${Math.min(100, d.used_percent)}%` }"
          />
        </div>
        <div class="disk-sub">{{ d.used }} / {{ d.total }}</div>
      </div>
    </div>
    <p v-else class="empty">暂无磁盘数据</p>
  </div>
</template>

<style scoped>
.disk-tab {
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.disk-list {
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.disk-item {
  background: var(--bg-elevated, #161c24);
  border: 1px solid var(--border-color);
  border-radius: var(--radius-md, 8px);
  padding: 8px 12px;
  display: flex;
  flex-direction: column;
  gap: 5px;
}
.disk-head {
  display: flex;
  justify-content: space-between;
  align-items: baseline;
}
.disk-mount {
  font-size: 12px;
  font-weight: 500;
  color: var(--text-primary);
  font-family: var(--font-mono, ui-monospace, monospace);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.disk-pct {
  font-size: 12px;
  font-weight: 600;
  color: var(--text-secondary);
  font-variant-numeric: tabular-nums;
  flex-shrink: 0;
}
.disk-pct.warn {
  color: var(--danger, #f87171);
}
.disk-bar {
  height: 6px;
  background: var(--bg-panel, #10141b);
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
.disk-sub {
  font-size: 11px;
  color: var(--text-tertiary);
  font-family: var(--font-mono, ui-monospace, monospace);
}
.empty {
  font-size: 12px;
  color: var(--text-tertiary);
  text-align: center;
  padding: 24px 0;
}
</style>
