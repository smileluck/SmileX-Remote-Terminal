<script setup lang="ts">
/**
 * TransferManager - 全局传输管理（UU远程式）
 *
 * - 右下角 FAB：活跃传输数徽标 + 总速度，点击打开抽屉
 * - 抽屉列表：按组展示进度/速度/状态，支持暂停/恢复/取消/重试、
 *   完成下载「在文件夹中显示」、清除已完成
 * - 数据来自 stores/transfer.ts（transfer_event 事件 upsert）
 */
import { computed } from 'vue'
import {
  NBadge,
  NButton,
  NDrawer,
  NDrawerContent,
  NEmpty,
  NIcon,
  NProgress,
  NTag,
  NTooltip,
  useMessage,
} from 'naive-ui'
import {
  ArrowsLeftRight,
  Download,
  Folder,
  PlayerPause,
  PlayerPlay,
  Refresh,
  Upload,
  X,
} from '@vicons/tabler'
import { revealItemInDir } from '@tauri-apps/plugin-opener'

import { useTransferStore } from '@/stores/transfer'
import type { TransferGroupInfo, TransferStatus } from '@/types/transfer'

const store = useTransferStore()
const message = useMessage()

/** 新任务在前 */
const list = computed<TransferGroupInfo[]>(() => [...store.groups].reverse())

const STATUS_META: Record<TransferStatus, { label: string; type: 'default' | 'info' | 'success' | 'warning' | 'error' }> = {
  queued: { label: '排队中', type: 'default' },
  running: { label: '传输中', type: 'info' },
  paused: { label: '已暂停', type: 'warning' },
  completed: { label: '已完成', type: 'success' },
  failed: { label: '失败', type: 'error' },
  canceled: { label: '已取消', type: 'default' },
}

function fmtBytes(n: number): string {
  if (n < 1024) return `${n} B`
  if (n < 1024 ** 2) return `${(n / 1024).toFixed(1)} KB`
  if (n < 1024 ** 3) return `${(n / 1024 ** 2).toFixed(1)} MB`
  return `${(n / 1024 ** 3).toFixed(2)} GB`
}

function fmtSpeed(bps: number): string {
  if (bps <= 0) return ''
  return `${fmtBytes(bps)}/s`
}

function percent(g: TransferGroupInfo): number | null {
  if (g.sizeTotal <= 0) return null
  return Math.min(100, Math.floor((g.bytesDone / g.sizeTotal) * 100))
}

async function openFolder(path: string) {
  try {
    await revealItemInDir(path)
  } catch (e) {
    message.error(`打开文件夹失败: ${e}`)
  }
}
</script>

<template>
  <!-- 右下角 FAB -->
  <div class="tm-fab" @click="store.managerVisible = true">
    <NBadge :value="store.activeCount" :max="99" :show="store.activeCount > 0">
      <NButton circle size="large" type="primary" secondary>
        <NIcon :component="ArrowsLeftRight" :size="20" />
      </NButton>
    </NBadge>
    <span v-if="store.totalSpeedBps > 0" class="tm-fab-speed">{{ fmtSpeed(store.totalSpeedBps) }}</span>
  </div>

  <NDrawer v-model:show="store.managerVisible" placement="right" :width="440" :z-index="300">
    <NDrawerContent title="传输管理" closable>
      <div v-if="list.length" class="tm-toolbar">
        <NButton size="tiny" quaternary @click="store.clearFinished()">清除已完成</NButton>
      </div>
      <div v-if="!list.length" class="tm-empty">
        <NEmpty size="small" description="暂无传输任务" />
      </div>
      <div v-for="g in list" :key="g.groupId" class="tm-row">
        <div class="tm-head">
          <NIcon
            :component="g.kind === 'upload' ? Upload : Download"
            class="tm-kind"
            :class="g.kind"
            :size="16"
          />
          <span class="tm-name" :title="g.name">{{ g.name }}</span>
          <NTag size="small" :type="STATUS_META[g.status].type" :bordered="false" round>
            {{ STATUS_META[g.status].label }}
          </NTag>
        </div>
        <div class="tm-meta">
          <span class="tm-session">{{ g.sessionLabel ?? g.sessionId.slice(0, 8) }}</span>
          <span>{{ fmtBytes(g.bytesDone) }}<template v-if="g.sizeTotal > 0"> / {{ fmtBytes(g.sizeTotal) }}</template></span>
          <span v-if="g.isDir">{{ g.filesDone }}/{{ g.fileCount }} 个文件</span>
          <span v-if="g.status === 'running'" class="tm-speed">{{ fmtSpeed(g.speedBps) }}</span>
        </div>
        <NProgress
          :percentage="percent(g) ?? 0"
          :indeterminate="g.status === 'running' && percent(g) === null"
          :status="g.status === 'failed' ? 'error' : g.status === 'completed' ? 'success' : 'default'"
          :height="6"
          :border-radius="3"
          :show-indicator="false"
        />
        <div v-if="g.error" class="tm-error" :title="g.error">{{ g.error }}</div>
        <div class="tm-actions">
          <template v-if="g.status === 'running' || g.status === 'queued'">
            <NTooltip placement="top">
              <template #trigger>
                <NButton quaternary circle size="tiny" @click="store.pause(g.groupId)">
                  <NIcon :component="PlayerPause" />
                </NButton>
              </template>
              暂停
            </NTooltip>
            <NTooltip placement="top">
              <template #trigger>
                <NButton quaternary circle size="tiny" @click="store.cancel(g.groupId)">
                  <NIcon :component="X" />
                </NButton>
              </template>
              取消
            </NTooltip>
          </template>
          <template v-else-if="g.status === 'paused'">
            <NTooltip placement="top">
              <template #trigger>
                <NButton quaternary circle size="tiny" @click="store.resume(g.groupId)">
                  <NIcon :component="PlayerPlay" />
                </NButton>
              </template>
              恢复（断点续传）
            </NTooltip>
            <NTooltip placement="top">
              <template #trigger>
                <NButton quaternary circle size="tiny" @click="store.cancel(g.groupId)">
                  <NIcon :component="X" />
                </NButton>
              </template>
              取消
            </NTooltip>
          </template>
          <template v-else-if="g.status === 'failed' || g.status === 'canceled'">
            <NTooltip placement="top">
              <template #trigger>
                <NButton quaternary circle size="tiny" @click="store.retry(g.groupId)">
                  <NIcon :component="Refresh" />
                </NButton>
              </template>
              重试（断点续传）
            </NTooltip>
          </template>
          <NTooltip
            v-if="g.status === 'completed' && g.kind === 'download' && g.localPath"
            placement="top"
          >
            <template #trigger>
              <NButton quaternary circle size="tiny" @click="openFolder(g.localPath!)">
                <NIcon :component="Folder" />
              </NButton>
            </template>
            在文件夹中显示
          </NTooltip>
        </div>
      </div>
    </NDrawerContent>
  </NDrawer>
</template>

<style scoped>
.tm-fab {
  position: fixed;
  right: 20px;
  bottom: 20px;
  z-index: 200;
  display: flex;
  align-items: center;
  gap: 8px;
  cursor: pointer;
}
.tm-fab-speed {
  font-size: 12px;
  color: var(--primary);
  background: var(--bg-elevated);
  border: 1px solid var(--border-color);
  border-radius: 10px;
  padding: 1px 8px;
}
.tm-empty {
  padding: 48px 0;
}
.tm-toolbar {
  display: flex;
  justify-content: flex-end;
  padding-bottom: 4px;
}
.tm-row {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 10px 4px;
  border-bottom: 1px solid var(--border-color);
}
.tm-row:last-child {
  border-bottom: none;
}
.tm-head {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}
.tm-kind {
  flex-shrink: 0;
  color: var(--text-tertiary);
}
.tm-kind.upload {
  color: var(--primary);
}
.tm-kind.download {
  color: var(--kind-rdp-fg, #34d399);
}
.tm-name {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 13px;
  color: var(--text-primary);
}
.tm-meta {
  display: flex;
  align-items: center;
  gap: 10px;
  font-size: 11px;
  color: var(--text-tertiary);
}
.tm-session {
  max-width: 140px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.tm-speed {
  color: var(--primary);
}
.tm-error {
  font-size: 11px;
  color: var(--error-color, #f87171);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.tm-actions {
  display: flex;
  gap: 2px;
}
</style>
