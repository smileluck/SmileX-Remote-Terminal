<script setup lang="ts">
/**
 * FilePanel - SFTP 文件面板
 *
 * 终端右侧可折叠抽屉：远端目录浏览 / 上传（对话框 + 拖拽）/ 下载 /
 * 新建目录 / 递归删除 / 重命名。传输任务入全局队列，
 * 进度与继续/暂停/取消/重试在面板底部传输区直接查看操作。
 * 由 TerminalView 持有（tab 须带 sessionId）。
 */
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import {
  NButton,
  NDropdown,
  NIcon,
  NInput,
  NSpin,
  NEmpty,
  NTooltip,
  useDialog,
  useMessage,
} from 'naive-ui'
import {
  Folder,
  File,
  Refresh,
  Upload,
  Download,
  Trash,
  Edit,
  FolderPlus,
  ChevronRight,
} from '@vicons/tabler'
import { open as openFileDialog } from '@tauri-apps/plugin-dialog'
import { getCurrentWebview } from '@tauri-apps/api/webview'

import * as sftp from '@/services/sftp'
import type { SftpEntry } from '@/services/sftp'
import { useTransferStore } from '@/stores/transfer'
import { useTabsStore } from '@/stores/tabs'
import { useLayoutStore } from '@/stores/layout'
import TransferList from './TransferList.vue'

const props = defineProps<{ sessionId: string }>()
const message = useMessage()
const dialog = useDialog()
const transferStore = useTransferStore()
const tabsStore = useTabsStore()
const layout = useLayoutStore()

const loading = ref(false)
const entries = ref<SftpEntry[]>([])
/** 当前绝对路径（初始为远端 home，由后端返回的 entry.path 推断） */
const cwd = ref('/')
/** 拖拽悬停遮罩 */
const dragOver = ref(false)

/** 左边缘拖拽调宽（200–480px，持久化到 layout store） */
const resizing = ref(false)
function onResizeStart(e: MouseEvent) {
  resizing.value = true
  const startX = e.clientX
  const startWidth = layout.filesWidth
  const onMove = (ev: MouseEvent) => {
    // 向左拖增大宽度
    const w = Math.min(480, Math.max(200, startWidth + (startX - ev.clientX)))
    layout.filesWidth = w
  }
  const onUp = () => {
    resizing.value = false
    window.removeEventListener('mousemove', onMove)
    window.removeEventListener('mouseup', onUp)
  }
  window.addEventListener('mousemove', onMove)
  window.addEventListener('mouseup', onUp)
  e.preventDefault()
}

async function load(path?: string) {
  loading.value = true
  try {
    entries.value = await sftp.list(props.sessionId, path || cwd.value)
    if (path !== undefined) cwd.value = path
  } catch (e) {
    message.error(String(e))
  } finally {
    loading.value = false
  }
}

onMounted(async () => {
  loading.value = true
  try {
    // path 省略 → 后端进入远端 home；从返回条目 path 推断 cwd
    entries.value = await sftp.list(props.sessionId)
    cwd.value = entries.value[0]?.path.replace(/\/[^/]*$/, '') || '/'
  } catch (e) {
    message.error(String(e))
  } finally {
    loading.value = false
  }
  // 拖拽上传（webview 级事件，多面板时只响应激活 tab）
  try {
    unlistenDrag = await getCurrentWebview().onDragDropEvent((event) => {
      if (tabsStore.activeTab?.sessionId !== props.sessionId) return
      const payload = event.payload
      if (payload.type === 'enter') {
        if (payload.paths?.length) dragOver.value = true
      } else if (payload.type === 'over') {
        // over 变体不带 paths，视为悬停延续（enter 已确认携带文件）
      } else if (payload.type === 'drop') {
        dragOver.value = false
        enqueueUpload(payload.paths ?? [])
      } else {
        dragOver.value = false
      }
    })
  } catch {
    // 非 Tauri 环境（vite 预览）无此 API
  }
})

let unlistenDrag: (() => void) | null = null
onUnmounted(() => unlistenDrag?.())

/** 本会话已完成传输数：变化时自动刷新目录（上传完成后列表更新） */
const completedCount = computed(
  () =>
    transferStore.groups.filter(
      (g) => g.sessionId === props.sessionId && g.status === 'completed',
    ).length,
)
watch(completedCount, () => {
  if (completedCount.value > 0) load()
})

function enter(entry: SftpEntry) {
  if (!entry.is_dir) return
  load(entry.path)
}

function goCrumb(index: number) {
  const parts = cwd.value.split('/').filter(Boolean)
  const target = '/' + parts.slice(0, index + 1).join('/')
  load(target || '/')
}

/** 入队上传并反馈 */
async function enqueueUpload(localPaths: string[]) {
  if (!localPaths.length) return
  try {
    await transferStore.startUpload(props.sessionId, localPaths, cwd.value)
    message.success(`已加入上传队列（${localPaths.length} 项）`)
  } catch (e) {
    message.error(String(e))
  }
}

/** 对话框选择文件上传（多选） */
async function pickFiles() {
  try {
    const res = await openFileDialog({ multiple: true, title: '选择上传的文件' })
    enqueueUpload(Array.isArray(res) ? res : res ? [res] : [])
  } catch (e) {
    message.error(String(e))
  }
}

/** 对话框选择文件夹上传（递归） */
async function pickFolder() {
  try {
    const res = await openFileDialog({ directory: true, title: '选择上传的文件夹' })
    enqueueUpload(res ? [res] : [])
  } catch (e) {
    message.error(String(e))
  }
}

/** 入队下载（文件或递归文件夹） */
async function handleDownload(entry: SftpEntry) {
  try {
    await transferStore.startDownload(props.sessionId, [entry.path])
    message.success(`已加入下载队列：${entry.name}`)
  } catch (e) {
    message.error(String(e))
  }
}

/** 递归删除（危险操作，弹确认） */
function handleRemove(entry: SftpEntry) {
  dialog.warning({
    title: entry.is_dir ? '删除目录' : '删除文件',
    content: `将删除 ${entry.path}${entry.is_dir ? ' 及其全部内容' : ''}，不可恢复。`,
    positiveText: '删除',
    negativeText: '取消',
    onPositiveClick: async () => {
      try {
        await sftp.remove(props.sessionId, entry.path)
        message.success(`已删除 ${entry.name}`)
        load()
      } catch (e) {
        message.error(String(e))
      }
    },
  })
}

let renaming: SftpEntry | null = null
const renameValue = ref('')
const renamingIndex = ref(-1)

function startRename(entry: SftpEntry) {
  renaming = entry
  renameValue.value = entry.name
  renamingIndex.value = entries.value.indexOf(entry)
}

async function confirmRename() {
  if (!renaming || !renameValue.value || renameValue.value === renaming.name) {
    renamingIndex.value = -1
    return
  }
  const parent = renaming.path.slice(0, renaming.path.lastIndexOf('/'))
  try {
    await sftp.rename(props.sessionId, renaming.path, `${parent}/${renameValue.value}`)
    message.success('已重命名')
    load()
  } catch (e) {
    message.error(String(e))
  } finally {
    renamingIndex.value = -1
  }
}

const newDirName = ref('')
const creatingDir = ref(false)

async function confirmMkdir() {
  if (!newDirName.value) return
  try {
    await sftp.mkdir(props.sessionId, `${cwd.value}/${newDirName.value}`)
    message.success('目录已创建')
    creatingDir.value = false
    newDirName.value = ''
    load()
  } catch (e) {
    message.error(String(e))
  }
}

/** 右键菜单（手动定位：单实例挂面板根节点） */
const menuShow = ref(false)
const menuX = ref(0)
const menuY = ref(0)
let menuTarget: SftpEntry | null = null

function onContextMenu(e: MouseEvent, entry: SftpEntry) {
  menuTarget = entry
  menuX.value = e.clientX
  menuY.value = e.clientY
  menuShow.value = true
}

function onMenuSelect(key: string) {
  menuShow.value = false
  if (menuTarget) onRowMenu(key, menuTarget)
}

function rowMenuOptions(entry: SftpEntry) {
  return [
    { label: entry.is_dir ? '下载（递归）' : '下载', key: 'download' },
    { label: '重命名', key: 'rename' },
    { label: entry.is_dir ? '删除（递归）' : '删除', key: 'remove' },
  ]
}

function onRowMenu(key: string, entry: SftpEntry) {
  if (key === 'download') handleDownload(entry)
  else if (key === 'rename') startRename(entry)
  else if (key === 'remove') handleRemove(entry)
}

function fmtSize(n: number): string {
  if (n < 1024) return `${n} B`
  if (n < 1024 ** 2) return `${(n / 1024).toFixed(1)} KB`
  if (n < 1024 ** 3) return `${(n / 1024 ** 2).toFixed(1)} MB`
  return `${(n / 1024 ** 3).toFixed(1)} GB`
}

defineExpose({ reload: load })
</script>

<template>
  <div class="file-panel" :style="{ width: layout.filesWidth + 'px' }">
    <div class="resize-handle" :class="{ dragging: resizing }" @mousedown="onResizeStart" />
    <div class="fp-toolbar">
      <div class="fp-crumbs">
        <span class="crumb" @click="load('/')">/</span>
        <template v-for="(c, i) in cwd.split('/').filter(Boolean)" :key="i">
          <NIcon :component="ChevronRight" class="crumb-sep" />
          <span class="crumb" @click="goCrumb(i)">{{ c }}</span>
        </template>
      </div>
      <div class="fp-actions">
        <NTooltip placement="bottom">
          <template #trigger>
            <NButton quaternary circle size="tiny" @click="load()">
              <NIcon :component="Refresh" />
            </NButton>
          </template>
          刷新
        </NTooltip>
        <NTooltip placement="bottom">
          <template #trigger>
            <NButton quaternary circle size="tiny" @click="creatingDir = !creatingDir">
              <NIcon :component="FolderPlus" />
            </NButton>
          </template>
          新建目录
        </NTooltip>
        <NTooltip placement="bottom">
          <template #trigger>
            <NButton quaternary circle size="tiny" @click="pickFiles">
              <NIcon :component="Upload" />
            </NButton>
          </template>
          上传文件
        </NTooltip>
        <NTooltip placement="bottom">
          <template #trigger>
            <NButton quaternary circle size="tiny" @click="pickFolder">
              <NIcon :component="Folder" />
            </NButton>
          </template>
          上传文件夹
        </NTooltip>
      </div>
    </div>

    <div v-if="creatingDir" class="fp-mkdir">
      <NInput
        v-model:value="newDirName"
        size="tiny"
        placeholder="目录名"
        @keyup.enter="confirmMkdir"
      />
      <NButton size="tiny" type="primary" @click="confirmMkdir">创建</NButton>
    </div>

    <NSpin :show="loading" size="small" class="fp-body">
      <div v-if="!entries.length && !loading" class="fp-empty">
        <NEmpty size="small" description="空目录" />
      </div>
      <div
        v-for="(entry, i) in entries"
        :key="entry.path"
        class="fp-row"
        @dblclick="enter(entry)"
        @contextmenu.prevent="(e: MouseEvent) => onContextMenu(e, entry)"
      >
        <NIcon
          :component="entry.is_dir ? Folder : File"
          class="fp-icon"
          :class="{ dir: entry.is_dir }"
        />
        <template v-if="renamingIndex === i">
          <NInput
            v-model:value="renameValue"
            size="tiny"
            autofocus
            @keyup.enter="confirmRename"
            @blur="confirmRename"
          />
        </template>
        <template v-else>
          <span class="fp-name" :title="entry.name" @click="enter(entry)">{{ entry.name }}</span>
          <span class="fp-size">{{ entry.is_dir ? '—' : fmtSize(entry.size) }}</span>
          <span class="fp-ops">
            <NButton quaternary circle size="tiny" @click.stop="handleDownload(entry)">
              <NIcon :component="Download" />
            </NButton>
            <NButton quaternary circle size="tiny" @click.stop="startRename(entry)">
              <NIcon :component="Edit" />
            </NButton>
            <NButton quaternary circle size="tiny" @click.stop="handleRemove(entry)">
              <NIcon :component="Trash" />
            </NButton>
          </span>
        </template>
      </div>
      <!-- 拖拽上传遮罩 -->
      <div v-if="dragOver" class="fp-dropzone">
        <NIcon :component="Upload" :size="28" />
        <span>松开以上传到 {{ cwd }}</span>
      </div>
    </NSpin>

    <!-- 传输区：文件列表下方，仅有任务时显示（暂停/失败可一键继续） -->
    <div v-if="transferStore.groups.length" class="fp-transfers">
      <div class="fp-transfers-head">
        <span class="fp-transfers-title">传输 · {{ transferStore.groups.length }}</span>
        <NButton size="tiny" quaternary @click="transferStore.clearFinished()">清除已完成</NButton>
      </div>
      <div class="fp-transfers-body">
        <TransferList :show-toolbar="false" />
      </div>
    </div>

    <!-- 右键菜单（跟随光标位置） -->
    <NDropdown
      trigger="manual"
      :show="menuShow"
      :x="menuX"
      :y="menuY"
      placement="bottom-start"
      :options="menuTarget ? rowMenuOptions(menuTarget) : []"
      @select="onMenuSelect"
      @clickoutside="menuShow = false"
    />
  </div>
</template>

<style scoped>
.file-panel {
  display: flex;
  flex-direction: column;
  flex-shrink: 0;
  border-left: 1px solid var(--border-color);
  background: var(--bg-sidebar);
  overflow: hidden;
  position: relative;
}
.resize-handle {
  position: absolute;
  left: 0;
  top: 0;
  bottom: 0;
  width: 4px;
  cursor: col-resize;
  z-index: 10;
}
.resize-handle:hover,
.resize-handle.dragging {
  background: var(--primary);
  opacity: 0.6;
}
.fp-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 6px 8px;
  border-bottom: 1px solid var(--border-color);
  gap: 4px;
}
.fp-crumbs {
  display: flex;
  align-items: center;
  gap: 2px;
  min-width: 0;
  overflow: hidden;
  font-size: 12px;
}
.crumb {
  color: var(--text-secondary);
  cursor: pointer;
  white-space: nowrap;
  padding: 0 2px;
}
.crumb:hover {
  color: var(--primary);
}
.crumb-sep {
  font-size: 11px;
  color: var(--text-tertiary);
  flex-shrink: 0;
}
.fp-actions {
  display: flex;
  gap: 2px;
  flex-shrink: 0;
}
.fp-mkdir {
  display: flex;
  gap: 4px;
  padding: 6px 8px;
  border-bottom: 1px solid var(--border-color);
}
.fp-body {
  flex: 1;
  overflow-y: auto;
  padding: 4px 0;
  position: relative;
}
.fp-empty {
  padding: 24px 0;
}
.fp-row {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 4px 8px;
  font-size: 12px;
  cursor: default;
}
.fp-row:hover {
  background: var(--bg-elevated);
}
.fp-icon {
  font-size: 14px;
  color: var(--text-tertiary);
  flex-shrink: 0;
}
.fp-icon.dir {
  color: var(--kind-rdp-fg);
}
.fp-name {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--text-primary);
  cursor: pointer;
}
.fp-size {
  color: var(--text-tertiary);
  font-size: 11px;
  flex-shrink: 0;
}
.fp-ops {
  display: none;
  gap: 0;
  flex-shrink: 0;
}
.fp-row:hover .fp-ops {
  display: flex;
}
.fp-row:hover .fp-size {
  display: none;
}
.fp-dropzone {
  position: absolute;
  inset: 0;
  z-index: 10;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 8px;
  background: color-mix(in srgb, var(--primary) 12%, var(--bg-sidebar));
  border: 2px dashed var(--primary);
  color: var(--primary);
  font-size: 13px;
  pointer-events: none;
}
/* 底部传输区：限高滚动，不挤压文件列表 */
.fp-transfers {
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  max-height: 240px;
  border-top: 1px solid var(--border-color);
}
.fp-transfers-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 2px 8px;
  font-size: 11px;
  color: var(--text-secondary);
  border-bottom: 1px solid var(--border-color);
  flex-shrink: 0;
}
.fp-transfers-title {
  font-weight: 600;
}
.fp-transfers-body {
  min-height: 0;
  overflow-y: auto;
  padding: 0 8px;
}
</style>
