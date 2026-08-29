<script setup lang="ts">
/**
 * FilePanel - SFTP 文件面板
 *
 * 终端右侧可折叠抽屉：远端目录浏览 / 上传 / 下载 / 新建目录 / 删除 / 重命名。
 * 由 TerminalView 持有（tab 须带 sessionId）。
 */
import { ref, onMounted } from 'vue'
import {
  NButton,
  NIcon,
  NInput,
  NSpin,
  NEmpty,
  NTooltip,
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
import * as sftp from '@/services/sftp'
import type { SftpEntry } from '@/services/sftp'

const props = defineProps<{ sessionId: string }>()
const message = useMessage()

const loading = ref(false)
const entries = ref<SftpEntry[]>([])
/** 当前绝对路径（初始为远端 home，由后端返回的 entry.path 推断） */
const cwd = ref('/')
const fileInput = ref<HTMLInputElement | null>(null)

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

async function handleDownload(entry: SftpEntry) {
  try {
    const saved = await sftp.download(props.sessionId, entry.path)
    message.success(`已下载到 ${saved}`)
  } catch (e) {
    message.error(String(e))
  }
}

async function handleRemove(entry: SftpEntry) {
  try {
    await sftp.remove(props.sessionId, entry.path, entry.is_dir)
    message.success(`已删除 ${entry.name}`)
    load()
  } catch (e) {
    message.error(String(e))
  }
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

async function handleUpload(ev: Event) {
  const input = ev.target as HTMLInputElement
  const file = input.files?.[0]
  if (!file) return
  try {
    const buf = new Uint8Array(await file.arrayBuffer())
    await sftp.upload(props.sessionId, `${cwd.value}/${file.name}`, buf)
    message.success(`已上传 ${file.name}`)
    load()
  } catch (e) {
    message.error(String(e))
  } finally {
    input.value = ''
  }
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
  <div class="file-panel">
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
            <NButton quaternary circle size="tiny" @click="fileInput?.click()">
              <NIcon :component="Upload" />
            </NButton>
          </template>
          上传文件
        </NTooltip>
        <input ref="fileInput" type="file" hidden @change="handleUpload" />
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
            <NButton
              v-if="!entry.is_dir"
              quaternary
              circle
              size="tiny"
              @click.stop="handleDownload(entry)"
            >
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
    </NSpin>
  </div>
</template>

<style scoped>
.file-panel {
  display: flex;
  flex-direction: column;
  width: 240px;
  flex-shrink: 0;
  border-left: 1px solid var(--border-color);
  background: var(--bg-sidebar);
  overflow: hidden;
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
.crumb-rename {
  width: 90px;
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
</style>
