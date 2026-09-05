<script setup lang="ts">
/**
 * TransferList - 传输任务列表（可复用）
 *
 * 文件面板底部传输区与传输管理抽屉共用：按组展示进度/速度/状态，
 * 支持暂停/恢复/取消/重试（断点续传）、完成下载「在文件夹中显示」、
 * 清除已完成。已暂停/失败任务行附醒目提示并突出「继续/重试」入口。
 * 数据来自 stores/transfer.ts（transfer_event 事件 upsert）。
 */
import { computed } from 'vue'
import { NButton, NEmpty, NIcon, NProgress, NTag, NTooltip, useMessage } from 'naive-ui'
import { Download, Folder, PlayerPause, PlayerPlay, Refresh, Upload, X } from '@vicons/tabler'
import { revealItemInDir } from '@tauri-apps/plugin-opener'

import { useTransferStore } from '@/stores/transfer'
import type { TransferGroupInfo, TransferStatus } from '@/types/transfer'

withDefaults(defineProps<{ showToolbar?: boolean }>(), { showToolbar: true })

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
  <div class="tl-wrap">
    <div v-if="showToolbar && list.length" class="tl-toolbar">
      <NButton size="tiny" quaternary @click="store.clearFinished()">清除已完成</NButton>
    </div>
    <div v-if="!list.length" class="tl-empty">
      <NEmpty size="small" description="暂无传输任务" />
    </div>
    <div
      v-for="g in list"
      :key="g.groupId"
      class="tl-row"
      :class="{ paused: g.status === 'paused' }"
    >
      <div class="tl-head">
        <NIcon
          :component="g.kind === 'upload' ? Upload : Download"
          class="tl-kind"
          :class="g.kind"
          :size="16"
        />
        <span class="tl-name" :title="g.name">{{ g.name }}</span>
        <NTag size="small" :type="STATUS_META[g.status].type" :bordered="false" round>
          {{ STATUS_META[g.status].label }}
        </NTag>
      </div>
      <div class="tl-meta">
        <span class="tl-session">{{ g.sessionLabel ?? g.sessionId.slice(0, 8) }}</span>
        <span>{{ fmtBytes(g.bytesDone) }}<template v-if="g.sizeTotal > 0"> / {{ fmtBytes(g.sizeTotal) }}</template></span>
        <span v-if="g.isDir">{{ g.filesDone }}/{{ g.fileCount }} 个文件</span>
        <span v-if="g.status === 'running'" class="tl-speed">{{ fmtSpeed(g.speedBps) }}</span>
      </div>
      <NProgress
        :percentage="percent(g) ?? 0"
        :indeterminate="g.status === 'running' && percent(g) === null"
        :status="g.status === 'failed' ? 'error' : g.status === 'completed' ? 'success' : 'default'"
        :height="6"
        :border-radius="3"
        :show-indicator="false"
      />
      <!-- 暂停/失败醒目提示：一键继续断点续传 -->
      <div v-if="g.status === 'paused'" class="tl-hint warning">已暂停 · 点击「继续」断点续传</div>
      <div v-if="g.status === 'failed'" class="tl-hint error" :title="g.error || undefined">
        {{ g.error || '传输失败' }}（点击「重试」继续，断点续传）
      </div>
      <div class="tl-actions">
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
              <NButton type="primary" quaternary circle size="tiny" @click="store.resume(g.groupId)">
                <NIcon :component="PlayerPlay" />
              </NButton>
            </template>
            继续（断点续传）
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
              <NButton type="primary" quaternary circle size="tiny" @click="store.retry(g.groupId)">
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
  </div>
</template>

<style scoped>
.tl-toolbar {
  display: flex;
  justify-content: flex-end;
  padding-bottom: 4px;
}
.tl-empty {
  padding: 32px 0;
}
.tl-row {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 10px 4px;
  border-bottom: 1px solid var(--border-color);
}
.tl-row:last-child {
  border-bottom: none;
}
.tl-row.paused {
  background: color-mix(in srgb, var(--warning, #fbbf24) 7%, transparent);
}
.tl-head {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}
.tl-kind {
  flex-shrink: 0;
  color: var(--text-tertiary);
}
.tl-kind.upload {
  color: var(--primary);
}
.tl-kind.download {
  color: var(--kind-rdp-fg, #34d399);
}
.tl-name {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 13px;
  color: var(--text-primary);
}
.tl-meta {
  display: flex;
  align-items: center;
  gap: 10px;
  font-size: 11px;
  color: var(--text-tertiary);
}
.tl-session {
  max-width: 140px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.tl-speed {
  color: var(--primary);
}
.tl-hint {
  font-size: 11px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.tl-hint.warning {
  color: var(--warning, #fbbf24);
}
.tl-hint.error {
  color: var(--error-color, #f87171);
}
.tl-actions {
  display: flex;
  gap: 2px;
}
</style>
