<script setup lang="ts">
/**
 * FilePanel - SFTP 文件面板
 *
 * 终端右侧可折叠抽屉：远端目录浏览 / 上传（对话框 + 拖拽）/ 下载 /
 * 新建目录 / 递归删除 / 重命名。传输任务入全局队列，
 * 进度与继续/暂停/取消/重试在面板底部传输区直接查看操作。
 * 由 TerminalView 持有（tab 须带 sessionId）。
 * 外部导航：navPath prop（layout.openFilesAt 写入的一次性目标路径），
 * 消费后清除 layout.filesNavPath。
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
  NModal,
  NCheckbox,
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
  List,
  ListDetails,
} from '@vicons/tabler'
import { open as openFileDialog } from '@tauri-apps/plugin-dialog'
import { getCurrentWebview } from '@tauri-apps/api/webview'

import * as sftp from '@/services/sftp'
import type { SftpEntry } from '@/services/sftp'
import * as sessionService from '@/services/session'
import { connectProfile } from '@/composables/useSshConnect'
import { shellQuote } from '@/utils/shell'
import { fmtTime, fmtMode } from '@/utils/format'
import { useTransferStore } from '@/stores/transfer'
import { useTabsStore } from '@/stores/tabs'
import { useLayoutStore } from '@/stores/layout'
import { useSnippetsStore } from '@/stores/snippets'
import { useProfilesStore } from '@/stores/profiles'
import { useMonitorStore } from '@/stores/monitor'
import TransferList from './TransferList.vue'

const props = defineProps<{ sessionId: string; navPath?: string | null }>()
const emit = defineEmits<{ (e: 'open-split-at', path: string): void }>()
const message = useMessage()
const dialog = useDialog()
const transferStore = useTransferStore()
const tabsStore = useTabsStore()
const layout = useLayoutStore()
const snippets = useSnippetsStore()
const profiles = useProfilesStore()
const monitor = useMonitorStore()

const encoder = new TextEncoder()

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

/** 外部导航（layout.openFilesAt 写入）：一次性定位到目标路径，消费后清除 */
// 本地标志：watcher 消费 navPath 时会同步清空 store，onMounted 时 prop 可能
// 已被父组件更新为 null，不能再用 props.navPath 判断是否跳过默认 home 加载
let navHandled = false
watch(
  () => props.navPath,
  (p) => {
    if (!p) return
    // 多 tab 并存时只有活跃会话的面板消费导航目标（其余等待自己的目标）
    if (tabsStore.activeTab?.sessionId !== props.sessionId) return
    navHandled = true
    layout.filesNavPath = null
    void load(p)
  },
  { immediate: true },
)

onMounted(async () => {
  // 有外部导航目标时跳过默认 home 加载（navPath watch 已按目标路径加载）
  if (!navHandled) {
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

/** 第 index 段 crumb 对应的绝对路径（根为 index=-1 → '/'） */
function crumbPath(index: number): string {
  const parts = cwd.value.split('/').filter(Boolean)
  if (index < 0) return '/'
  return '/' + parts.slice(0, index + 1).join('/')
}

function goCrumb(index: number) {
  load(crumbPath(index))
}

/** 双击面包屑进入路径编辑：回车跳转（失败保留原状），Esc/失焦取消 */
const pathEditing = ref(false)
const pathDraft = ref('')

function startPathEdit() {
  pathDraft.value = cwd.value
  pathEditing.value = true
}

async function confirmPathEdit() {
  // Enter 确认后输入框卸载会再触发一次 blur，需去重
  if (!pathEditing.value) return
  const draft = pathDraft.value.trim()
  pathEditing.value = false
  if (!draft || draft === cwd.value) return
  // 不以 / 开头视为相对当前目录
  const target = draft.startsWith('/') ? draft : `${cwd.value.replace(/\/$/, '')}/${draft}`
  // 失败时 load 内部已 toast 且 cwd 不变，面包屑自然保持原路径
  await load(target)
}

/** 面包屑最多直接展示的层数（超出时折叠中间段为省略号，保留首尾） */
const MAX_CRUMBS = 3

/** 面包屑段：目录层级过深时显示首尾，中间用省略号替代（ellipsis 段 i=-1，不可点击） */
const crumbSegs = computed(() => {
  const parts = cwd.value.split('/').filter(Boolean)
  if (parts.length <= MAX_CRUMBS) return parts.map((name, i) => ({ name, i }))
  const head = parts.slice(0, 1).map((name, i) => ({ name, i }))
  const tail = parts.slice(-2).map((name, j) => ({ name, i: parts.length - 2 + j }))
  return [...head, { name: '…', i: -1 }, ...tail]
})

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
    { label: '授权（chmod）', key: 'chmod' },
    { label: entry.is_dir ? '删除（递归）' : '删除', key: 'remove' },
  ]
}

function onRowMenu(key: string, entry: SftpEntry) {
  if (key === 'download') handleDownload(entry)
  else if (key === 'rename') startRename(entry)
  else if (key === 'chmod') startChmod(entry)
  else if (key === 'remove') handleRemove(entry)
}

/* ---------------- 授权（chmod）对话框 ---------------- */

const chmodTarget = ref<SftpEntry | null>(null)
/** 模式位（仅低 9 位 rwxrwxrwx 参与编辑） */
const chmodBits = ref(0o644)
/** 八进制文本（与 chmodBits 双向同步） */
const chmodOctal = ref('644')
const chmodSaving = ref(false)

const CHMOD_GROUPS = [
  { label: '所有者', shift: 6 },
  { label: '用户组', shift: 3 },
  { label: '其他', shift: 0 },
] as const
const CHMOD_PERMS = [
  { label: '读', bit: 4 },
  { label: '写', bit: 2 },
  { label: '执行', bit: 1 },
] as const

function startChmod(entry: SftpEntry) {
  chmodTarget.value = entry
  const bits =
    entry.permissions != null ? entry.permissions & 0o777 : entry.is_dir ? 0o755 : 0o644
  chmodBits.value = bits
  chmodOctal.value = bits.toString(8).padStart(3, '0')
}

function chmodBit(shift: number, bit: number): boolean {
  return (chmodBits.value & (bit << shift)) !== 0
}

function setChmodBit(shift: number, bit: number, on: boolean) {
  const mask = bit << shift
  chmodBits.value = on ? chmodBits.value | mask : chmodBits.value & ~mask
  chmodOctal.value = chmodBits.value.toString(8).padStart(3, '0')
}

/** 八进制输入：合法（3 位 0-7）时同步到勾选 */
function onOctalInput(v: string) {
  chmodOctal.value = v
  if (/^[0-7]{3}$/.test(v)) chmodBits.value = parseInt(v, 8)
}

async function confirmChmod() {
  const target = chmodTarget.value
  if (!target) return
  chmodSaving.value = true
  try {
    await sftp.chmod(props.sessionId, target.path, chmodBits.value)
    message.success(`已将 ${target.name} 权限改为 ${chmodOctal.value}`)
    chmodTarget.value = null
    load()
  } catch (e) {
    message.error(String(e))
  } finally {
    chmodSaving.value = false
  }
}

/** 面包屑右键菜单（手动定位，与文件行菜单相互独立） */
const crumbMenuShow = ref(false)
const crumbMenuX = ref(0)
const crumbMenuY = ref(0)
/** 打开菜单时捕获的目标路径（菜单打开期间保持不变） */
const crumbMenuPath = ref('')

function onCrumbContextMenu(e: MouseEvent, path: string) {
  crumbMenuPath.value = path
  crumbMenuX.value = e.clientX
  crumbMenuY.value = e.clientY
  crumbMenuShow.value = true
}

/** 当前会话关联的档案（无则「新会话打开」不可用） */
const currentProfile = computed(() => {
  const tab = tabsStore.tabs.find((t) => t.sessionId === props.sessionId)
  return profiles.profiles.find((p) => p.id === tab?.profileId) ?? null
})

const crumbMenuOptions = computed(() => [
  { label: '复制绝对路径', key: 'copy' },
  { label: '保存到常用记录', key: 'save-snippet' },
  { label: '会话跳转到该路径', key: 'cd' },
  { label: '新会话打开该路径', key: 'new-session', disabled: !currentProfile.value },
  { label: '新分屏打开该路径', key: 'new-split' },
])

function onCrumbMenuSelect(key: string) {
  crumbMenuShow.value = false
  const path = crumbMenuPath.value
  if (!path) return
  switch (key) {
    case 'copy':
      void copyCrumbPath(path)
      break
    case 'save-snippet':
      void saveCrumbSnippet(path)
      break
    case 'cd':
      void cdToPath(props.sessionId, path)
      break
    case 'new-session':
      void openSessionAtPath(path)
      break
    case 'new-split':
      emit('open-split-at', path)
      break
  }
}

async function copyCrumbPath(path: string) {
  try {
    await navigator.clipboard.writeText(path)
    message.success('已复制路径')
  } catch {
    message.warning('复制失败：无法访问剪贴板')
  }
}

/** 保存为常用记录（cd 命令片段，未分组） */
async function saveCrumbSnippet(path: string) {
  const command = `cd ${shellQuote(path)}`
  try {
    await snippets.load()
    await snippets.save({
      id: crypto.randomUUID(),
      name: command.slice(0, 30),
      command,
      tags: '',
      groupName: '',
      sortOrder: 0,
      kind: 'command',
      checkCmd: '',
      createdAt: Math.floor(Date.now() / 1000),
    })
    message.success('已添加到常用记录')
  } catch (e) {
    message.error(`添加失败：${e}`)
  }
}

/** 向指定会话发送 cd（raw input，用户在终端可见命令与报错） */
async function cdToPath(sessionId: string, path: string) {
  try {
    await sessionService.input(sessionId, encoder.encode(`cd ${shellQuote(path)}\r`))
  } catch (e) {
    message.error(`发送失败：${e}`)
  }
}

/** 新建会话并落在指定路径（shell 就绪无信号，延时后发送 cd） */
async function openSessionAtPath(path: string) {
  const profile = currentProfile.value
  if (!profile) return
  try {
    const sid = await connectProfile(profile)
    tabsStore.addTab(profile.kind, profile.name, sid, profile.id)
    monitor.setActive(sid)
    setTimeout(() => {
      sessionService.input(sid, encoder.encode(`cd ${shellQuote(path)}\r`)).catch(() => {})
    }, 500)
  } catch (e) {
    message.error(`连接失败：${e}`)
  }
}

function fmtSize(n: number): string {
  if (n < 1024) return `${n} B`
  if (n < 1024 ** 2) return `${(n / 1024).toFixed(1)} KB`
  if (n < 1024 ** 3) return `${(n / 1024 ** 2).toFixed(1)} MB`
  return `${(n / 1024 ** 3).toFixed(1)} GB`
}

/** 详细视图 meta 行：大小 · 修改时间 · 权限（符号串 + 八进制） */
function metaLine(entry: SftpEntry): string {
  const size = entry.is_dir ? '—' : fmtSize(entry.size)
  const time = entry.mtime ? fmtTime(entry.mtime) : '—'
  const perm =
    entry.permissions != null
      ? `${fmtMode(entry.permissions)} ${(entry.permissions & 0o777).toString(8).padStart(3, '0')}`
      : '—'
  return `${size} · ${time} · ${perm}`
}

defineExpose({ reload: load })
</script>

<template>
  <div class="file-panel" :style="{ width: layout.filesWidth + 'px' }">
    <div class="resize-handle" :class="{ dragging: resizing }" @mousedown="onResizeStart" />
    <div class="fp-toolbar" @contextmenu.prevent="(e: MouseEvent) => onCrumbContextMenu(e, cwd)">
      <div
        class="fp-crumbs"
        :title="cwd"
        @dblclick="startPathEdit"
      >
        <NInput
          v-if="pathEditing"
          v-model:value="pathDraft"
          size="tiny"
          autofocus
          class="fp-path-input"
          placeholder="输入路径，回车跳转"
          @keyup.enter="confirmPathEdit"
          @keyup.esc="pathEditing = false"
          @blur="confirmPathEdit"
        />
        <template v-else>
          <span
            class="crumb"
            @click="load('/')"
            @contextmenu.prevent.stop="(e: MouseEvent) => onCrumbContextMenu(e, '/')"
          >/</span>
          <template v-for="(s, k) in crumbSegs" :key="k">
            <NIcon :component="ChevronRight" class="crumb-sep" />
            <span v-if="s.i < 0" class="crumb crumb-ellipsis">{{ s.name }}</span>
            <span
              v-else
              class="crumb"
              @click="goCrumb(s.i)"
              @contextmenu.prevent.stop="(e: MouseEvent) => onCrumbContextMenu(e, crumbPath(s.i))"
            >{{ s.name }}</span>
          </template>
        </template>
      </div>
      <div class="fp-actions">
        <NTooltip placement="bottom">
          <template #trigger>
            <NButton
              quaternary
              circle
              size="tiny"
              @click="layout.filesView = layout.filesView === 'simple' ? 'detail' : 'simple'"
            >
              <NIcon :component="layout.filesView === 'simple' ? ListDetails : List" />
            </NButton>
          </template>
          {{ layout.filesView === 'simple' ? '切换到详细视图' : '切换到简洁视图' }}
        </NTooltip>
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
        :class="{ detail: layout.filesView === 'detail' }"
        @dblclick="enter(entry)"
        @contextmenu.prevent="(e: MouseEvent) => onContextMenu(e, entry)"
      >
        <div class="fp-line">
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
            <span v-if="layout.filesView === 'simple'" class="fp-size">
              {{ entry.is_dir ? '—' : fmtSize(entry.size) }}
            </span>
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
        <div v-if="layout.filesView === 'detail' && renamingIndex !== i" class="fp-meta" :title="metaLine(entry)">
          {{ metaLine(entry) }}
        </div>
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
    <!-- 面包屑右键菜单 -->
    <NDropdown
      trigger="manual"
      :show="crumbMenuShow"
      :x="crumbMenuX"
      :y="crumbMenuY"
      placement="bottom-start"
      :options="crumbMenuOptions"
      @select="onCrumbMenuSelect"
      @clickoutside="crumbMenuShow = false"
    />
    <!-- 授权（chmod）对话框 -->
    <NModal
      :show="chmodTarget !== null"
      preset="card"
      title="修改权限"
      style="width: 380px"
      :bordered="false"
      @update:show="(v: boolean) => { if (!v) chmodTarget = null }"
    >
      <div class="chmod-file" :title="chmodTarget?.path">{{ chmodTarget?.name }}</div>
      <div class="chmod-grid">
        <span class="chmod-head" />
        <span v-for="p in CHMOD_PERMS" :key="p.bit" class="chmod-head">{{ p.label }}</span>
        <template v-for="g in CHMOD_GROUPS" :key="g.shift">
          <span class="chmod-group">{{ g.label }}</span>
          <NCheckbox
            v-for="p in CHMOD_PERMS"
            :key="p.bit"
            :checked="chmodBit(g.shift, p.bit)"
            size="small"
            @update:checked="(on: boolean) => setChmodBit(g.shift, p.bit, on)"
          />
        </template>
      </div>
      <div class="chmod-octal">
        <span class="chmod-group">八进制</span>
        <NInput
          :value="chmodOctal"
          size="small"
          class="chmod-octal-input"
          placeholder="如 755"
          @update:value="onOctalInput"
        />
      </div>
      <template #footer>
        <div class="chmod-footer">
          <NButton size="small" @click="chmodTarget = null">取消</NButton>
          <NButton size="small" type="primary" :loading="chmodSaving" @click="confirmChmod">
            应用
          </NButton>
        </div>
      </template>
    </NModal>
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
  flex: 1;
  align-self: stretch;
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
.crumb-ellipsis {
  cursor: default;
  color: var(--text-tertiary);
}
.crumb-ellipsis:hover {
  color: var(--text-tertiary);
}
.crumb-sep {
  font-size: 11px;
  color: var(--text-tertiary);
  flex-shrink: 0;
}
.fp-path-input {
  flex: 1;
  min-width: 0;
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
.chmod-file {
  font-size: 12px;
  font-weight: 500;
  color: var(--text-primary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  margin-bottom: 10px;
}
.chmod-grid {
  display: grid;
  grid-template-columns: 56px repeat(3, 1fr);
  gap: 6px 12px;
  align-items: center;
  margin-bottom: 12px;
}
.chmod-head {
  font-size: 11px;
  color: var(--text-tertiary);
}
.chmod-group {
  font-size: 12px;
  color: var(--text-secondary);
  flex-shrink: 0;
}
.chmod-octal {
  display: flex;
  align-items: center;
  gap: 8px;
}
.chmod-octal-input {
  width: 90px;
}
.chmod-footer {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
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
/* 详细视图：两行式（首行 图标+名称+操作，次行 meta 信息） */
.fp-row.detail {
  flex-direction: column;
  align-items: stretch;
  gap: 1px;
}
.fp-line {
  display: flex;
  flex: 1;
  align-items: center;
  gap: 6px;
  min-width: 0;
}
.fp-meta {
  font-size: 11px;
  color: var(--text-tertiary);
  padding-left: 20px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
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
