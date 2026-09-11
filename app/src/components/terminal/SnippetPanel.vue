<script setup lang="ts">
/**
 * SnippetPanel - 右栏「常用记录」面板
 *
 * 命令片段按组分类管理（与 ⌘K 命令面板共用 command_snippets 数据）：
 * - 顶部「命令 / 服务」分段切换（NTabs segment），列表按当前 Tab 的 kind 过滤
 * - 分组折叠列表：组头（执行整组/重命名/删除/拖拽）、条目（单条执行/编辑/删除/拖拽）
 * - 拖拽排序：Pointer Events 自实现（Tauri 拦截 HTML5 DnD，参照 SideBar），
 *   支持组内排序、跨组移动、整组拖动，结束后统一 persistOrder 落库
 *   （基于完整数组重编 sortOrder，另一 kind 的相对顺序不受影响）
 * - 执行：整组顺序执行（仅当前 Tab 类型条目，等待每条完成，退出码非零中止）/
 *   单条执行，走终端标记协议（termExec），无活跃 SSH 会话时按钮置灰
 * - 服务条目：静默通道（sessionService.exec）轮询运行状态（10s），
 *   提供启动/停止/重启快捷操作（终端可见执行 systemctl）
 */
import { ref, computed, onMounted, onUnmounted, watch } from 'vue'
import {
  NButton,
  NIcon,
  NInput,
  NModal,
  NAutoComplete,
  NEmpty,
  NPopconfirm,
  NTabs,
  NTab,
  NDropdown,
  useMessage,
  useDialog,
} from 'naive-ui'
import {
  Plus,
  PlayerPlay,
  PlayerStop,
  Pencil,
  Trash,
  GripVertical,
  ChevronRight,
  CircleCheck,
  CircleX,
  Loader,
  Clock,
  Eye,
  Refresh,
} from '@vicons/tabler'
import { useSnippetsStore, type SnippetGroup, type ServiceStatus } from '@/stores/snippets'
import { useTabsStore } from '@/stores/tabs'
import { segmentTabThemeOverrides } from '@/components/common/segmentTabTheme'
import FavoriteDirPanel from '@/components/terminal/FavoriteDirPanel.vue'
import type { CommandSnippet, SnippetKind } from '@/services/snippets'

const store = useSnippetsStore()
const tabs = useTabsStore()
const message = useMessage()
const dialog = useDialog()

onMounted(async () => {
  await store.load()
  store.startStatusPolling()
})
onUnmounted(() => store.stopStatusPolling())

// 活跃会话切换时刷新一次服务状态
watch(
  () => tabs.activeTab?.sessionId,
  () => void store.refreshServiceStatus(),
)

/* ---------------- Tab 过滤视图 ---------------- */

/** 当前 Tab：命令 / 服务 / 目录 */
const activeKind = ref<SnippetKind | 'directory'>('command')

/** 目录 Tab 子组件引用（「新增目录」下拉触发其弹窗） */
const favDirPanelRef = ref<InstanceType<typeof FavoriteDirPanel> | null>(null)

/** 当前 Tab 可见的分组（仅含对应 kind 的条目；空组不显示） */
const visibleGroups = computed<SnippetGroup[]>(() =>
  store.groups
    .map((g) => ({ ...g, items: g.items.filter((s) => s.kind === activeKind.value) }))
    .filter((g) => g.items.length > 0),
)

/* ---------------- 分组折叠 ---------------- */

const collapsedKeys = ref<Set<string>>(new Set())

function isCollapsed(key: string) {
  return collapsedKeys.value.has(key)
}

function toggleCollapse(key: string) {
  const s = new Set(collapsedKeys.value)
  if (s.has(key)) s.delete(key)
  else s.add(key)
  collapsedKeys.value = s
}

/* ---------------- 新增 / 编辑条目 ---------------- */

const showForm = ref(false)
const editingId = ref<string | null>(null)
const draft = ref({
  name: '',
  command: '',
  group: '',
  kind: 'command' as SnippetKind,
  checkCmd: '',
})

/** 已有分组名（所属组自动补全候选，取当前 Tab 的组） */
const groupOptions = computed(() =>
  visibleGroups.value.filter((g) => g.key).map((g) => ({ label: g.key, value: g.key })),
)

/** 「新增」下拉选项 */
const createOptions = [
  { label: '新增命令', key: 'command' },
  { label: '新增服务', key: 'service' },
  { label: '新增目录', key: 'directory' },
]

/** 弹窗标题（类型由打开方式决定，编辑时 kind 不可改） */
const formTitle = computed(
  () =>
    `${editingId.value ? '编辑' : '新增'}${draft.value.kind === 'service' ? '服务' : '命令'}`,
)

function openCreate(kind: string | number) {
  if (kind === 'directory') {
    activeKind.value = 'directory'
    favDirPanelRef.value?.openCreate()
    return
  }
  const k = (kind === 'service' ? 'service' : 'command') as SnippetKind
  editingId.value = null
  draft.value = { name: '', command: '', group: '', kind: k, checkCmd: '' }
  showForm.value = true
}

function openEdit(s: CommandSnippet) {
  editingId.value = s.id
  draft.value = {
    name: s.name,
    command: s.command,
    group: s.groupName,
    kind: s.kind,
    checkCmd: s.checkCmd,
  }
  showForm.value = true
}

async function submitForm() {
  const command = draft.value.command.trim()
  if (!command) {
    message.warning(draft.value.kind === 'service' ? '请填写服务名' : '请填写命令')
    return
  }
  const old = editingId.value
    ? store.snippets.find((s) => s.id === editingId.value)
    : undefined
  const kind = draft.value.kind
  const snippet: CommandSnippet = {
    id: editingId.value ?? crypto.randomUUID(),
    name: draft.value.name.trim() || command.slice(0, 30),
    command,
    tags: old?.tags ?? '',
    groupName: draft.value.group.trim(),
    sortOrder: old?.sortOrder ?? 0,
    kind,
    checkCmd: kind === 'service' ? draft.value.checkCmd.trim() : '',
    createdAt: old?.createdAt ?? Math.floor(Date.now() / 1000),
  }
  try {
    await store.save(snippet)
    showForm.value = false
    message.success('已保存')
    // 跨 Tab 新增（如在「命令」Tab 选「新增服务」）时切到对应 Tab，让用户看到新条目
    activeKind.value = kind
    if (kind === 'service') void store.refreshServiceStatus()
  } catch (e) {
    message.error(String(e))
  }
}

/* ---------------- 分组操作 ---------------- */

const showRename = ref(false)
const renameKey = ref('')
const renameName = ref('')

function openRenameGroup(g: SnippetGroup) {
  renameKey.value = g.key
  renameName.value = g.key
  showRename.value = true
}

async function submitRename() {
  const to = renameName.value.trim()
  if (!to) {
    message.warning('请填写分组名')
    return
  }
  try {
    await store.renameGroup(renameKey.value, to)
    showRename.value = false
    message.success('分组已重命名')
  } catch (e) {
    message.error(String(e))
  }
}

function removeGroup(g: SnippetGroup) {
  dialog.warning({
    title: '删除分组',
    content: `删除分组「${g.name}」？组内 ${g.items.length} 条命令将移入「未分组」。`,
    positiveText: '删除',
    negativeText: '取消',
    onPositiveClick: async () => {
      try {
        await store.deleteGroup(g.key)
      } catch (e) {
        message.error(String(e))
      }
    },
  })
}

function removeOne(s: CommandSnippet) {
  dialog.warning({
    title: '删除命令',
    content: `删除「${s.name}」？`,
    positiveText: '删除',
    negativeText: '取消',
    onPositiveClick: async () => {
      try {
        await store.remove(s.id)
      } catch (e) {
        message.error(String(e))
      }
    },
  })
}

/* ---------------- 执行 ---------------- */

async function runOne(s: CommandSnippet) {
  try {
    await store.runOne(s)
  } catch (e) {
    message.error(String(e))
  }
}

async function runGroup(g: SnippetGroup) {
  try {
    // 仅从命令/服务 Tab 的分组列表触发，activeKind 在此必为 SnippetKind
    await store.runGroup(g.key, activeKind.value as SnippetKind)
    message.success(`分组「${g.name}」执行完成`)
  } catch (e) {
    message.error(String(e))
  }
}

/** 服务快捷操作（启动/停止/重启，终端可见执行 systemctl） */
async function svcAction(s: CommandSnippet, action: 'start' | 'stop' | 'restart') {
  try {
    await store.runServiceAction(s, action)
  } catch (e) {
    message.error(String(e))
  }
}

/** 服务状态（无活跃 SSH 会话时一律未知） */
function svcStatus(s: CommandSnippet): ServiceStatus {
  if (!store.canRun) return 'unknown'
  return store.serviceStatus[s.id] ?? 'unknown'
}

const svcStatusText: Record<ServiceStatus, string> = {
  active: '运行中',
  inactive: '已停止',
  unknown: '未知',
}

/** 条目执行状态图标（仅运行期间/之后出现） */
function stateIcon(id: string) {
  const st = store.runStates[id]
  if (st === 'running') return { icon: Loader, cls: 'st-running' }
  if (st === 'success') return { icon: CircleCheck, cls: 'st-success' }
  if (st === 'failed') return { icon: CircleX, cls: 'st-failed' }
  if (st === 'pending') return { icon: Clock, cls: 'st-pending' }
  return null
}

/* ---------------- 拖拽排序（Pointer Events 自实现） ---------------- */

/** 拖拽判定阈值（px）：位移内视为点击 */
const DRAG_THRESHOLD = 6

interface DragInfo {
  kind: 'item' | 'group'
  /** item：片段 id；group：分组键 */
  id: string
  label: string
  startX: number
  startY: number
  started: boolean
  ghost: HTMLElement | null
}

let drag: DragInfo | null = null

/** 拖拽源（置灰高亮用） */
const draggingId = ref<string | null>(null)
/** 条目落点指示（目标条目 id + 插到前/后） */
const dropItem = ref<{ id: string; before: boolean } | null>(null)
/** 组落点高亮（条目拖到组空白 = 追加组尾；整组拖动 = 目标组前/后） */
const dropGroup = ref<{ key: string; before: boolean } | null>(null)

function onItemPointerDown(e: PointerEvent, s: CommandSnippet) {
  if (e.button !== 0) return
  beginDrag(e, { kind: 'item', id: s.id, label: s.name })
}

function onGroupPointerDown(e: PointerEvent, g: SnippetGroup) {
  if (e.button !== 0) return
  beginDrag(e, { kind: 'group', id: g.key, label: `分组：${g.name}` })
}

function beginDrag(e: PointerEvent, info: Pick<DragInfo, 'kind' | 'id' | 'label'>) {
  e.preventDefault() // 阻止文本选中；不影响 click
  drag = { ...info, startX: e.clientX, startY: e.clientY, started: false, ghost: null }
  window.addEventListener('pointermove', onDragPointerMove)
  window.addEventListener('pointerup', onDragPointerUp)
  window.addEventListener('pointercancel', onDragPointerUp)
}

function createGhost(label: string): HTMLElement {
  const el = document.createElement('div')
  el.className = 'snippet-drag-ghost'
  el.textContent = label
  document.body.appendChild(el)
  return el
}

function onDragPointerMove(e: PointerEvent) {
  const d = drag
  if (!d) return
  if (!d.started) {
    if (Math.hypot(e.clientX - d.startX, e.clientY - d.startY) < DRAG_THRESHOLD) return
    d.started = true
    d.ghost = createGhost(d.label)
    document.body.classList.add('snippet-dragging')
    draggingId.value = d.id
  }
  if (d.ghost) {
    d.ghost.style.left = `${e.clientX + 12}px`
    d.ghost.style.top = `${e.clientY + 12}px`
  }

  const el = document.elementFromPoint(e.clientX, e.clientY)
  if (d.kind === 'item') {
    const row = el?.closest('[data-si]') as HTMLElement | null
    if (row && row.dataset.si !== d.id) {
      const r = row.getBoundingClientRect()
      dropItem.value = { id: row.dataset.si!, before: e.clientY < r.top + r.height / 2 }
      dropGroup.value = null
    } else {
      const grp = el?.closest('[data-sg]') as HTMLElement | null
      dropItem.value = null
      dropGroup.value = grp ? { key: grp.dataset.sg!, before: true } : null
    }
  } else {
    const gc = el?.closest('[data-sgc]') as HTMLElement | null
    if (gc && gc.dataset.sgc !== d.id) {
      const r = gc.getBoundingClientRect()
      dropGroup.value = { key: gc.dataset.sgc!, before: e.clientY < r.top + r.height / 2 }
    } else {
      dropGroup.value = null
    }
  }
}

function cleanupDrag() {
  window.removeEventListener('pointermove', onDragPointerMove)
  window.removeEventListener('pointerup', onDragPointerUp)
  window.removeEventListener('pointercancel', onDragPointerUp)
  drag?.ghost?.remove()
  document.body.classList.remove('snippet-dragging')
  drag = null
  draggingId.value = null
  dropItem.value = null
  dropGroup.value = null
}

function onDragPointerUp() {
  const d = drag
  const item = dropItem.value
  const group = dropGroup.value
  const started = d?.started ?? false
  cleanupDrag()
  if (!d || !started) return // 未过阈值：视为点击
  void applyDrop(d, item, group)
}

async function applyDrop(
  d: DragInfo,
  item: { id: string; before: boolean } | null,
  group: { key: string; before: boolean } | null,
) {
  try {
    if (d.kind === 'item') {
      if (item) {
        const g = visibleGroups.value.find((gr) => gr.items.some((s) => s.id === item.id))
        if (!g) return
        const idx = g.items.findIndex((s) => s.id === item.id)
        const beforeId = item.before ? item.id : (g.items[idx + 1]?.id ?? null)
        // 目标位置紧邻自身 = 位置不变
        if (beforeId === d.id) return
        await store.moveItem(d.id, g.key, beforeId)
      } else if (group) {
        // 拖到组头/组空白：追加到该组末尾
        await store.moveItem(d.id, group.key, null)
      }
    } else if (group && group.key !== d.id) {
      const keys = visibleGroups.value.map((g) => g.key)
      const ti = keys.indexOf(group.key)
      const beforeKey = group.before ? group.key : (keys[ti + 1] ?? null)
      if (beforeKey === d.id) return
      await store.moveGroup(d.id, beforeKey)
    }
  } catch (e) {
    message.error(`排序保存失败：${e}`)
    void store.load(true)
  }
}
</script>

<template>
  <section class="snippet-panel">
    <div class="sp-toolbar">
      <NTabs
        v-model:value="activeKind"
        type="segment"
        size="small"
        :animated="false"
        class="sp-tabs"
        :theme-overrides="segmentTabThemeOverrides"
      >
        <NTab name="command" tab="命令" />
        <NTab name="service" tab="服务" />
        <NTab name="directory" tab="目录" />
      </NTabs>
      <NDropdown trigger="click" :options="createOptions" @select="openCreate">
        <NButton size="tiny" quaternary circle title="新增">
          <template #icon><NIcon :component="Plus" /></template>
        </NButton>
      </NDropdown>
    </div>

    <FavoriteDirPanel v-if="activeKind === 'directory'" ref="favDirPanelRef" />

    <template v-else>
    <NEmpty
      v-if="!visibleGroups.length"
      size="small"
      :description="activeKind === 'service' ? '暂无服务记录，点上方「新增」添加' : '暂无常用命令，点上方「新增」添加'"
      class="sp-empty"
    />

    <div
      v-for="g in visibleGroups"
      :key="g.key || '__ungrouped__'"
      class="sp-group"
      :data-sgc="g.key"
      :class="{
        'drag-source': draggingId === g.key,
        'drop-before': dropGroup?.key === g.key && dropGroup.before,
        'drop-after': dropGroup?.key === g.key && !dropGroup.before,
      }"
    >
      <div class="sp-group-head" :data-sg="g.key" @click="toggleCollapse(g.key)">
        <span class="sp-drag" title="拖拽排序" @pointerdown="onGroupPointerDown($event, g)" @click.stop>
          <NIcon :component="GripVertical" :size="13" />
        </span>
        <NIcon class="sp-arrow" :class="{ open: !isCollapsed(g.key) }" :component="ChevronRight" :size="12" />
        <span class="sp-group-name">{{ g.name }}</span>
        <span class="sp-group-count">{{ g.items.length }}</span>
        <span class="sp-ops" @click.stop @pointerdown.stop>
          <button
            class="sp-op"
            title="顺序执行整组"
            :disabled="!store.canRun || store.running"
            @click="runGroup(g)"
          >
            <NIcon :component="PlayerPlay" :size="13" />
          </button>
          <button v-if="g.key" class="sp-op" title="重命名分组" @click="openRenameGroup(g)">
            <NIcon :component="Pencil" :size="13" />
          </button>
          <button v-if="g.key" class="sp-op danger" title="删除分组（条目移入未分组）" @click="removeGroup(g)">
            <NIcon :component="Trash" :size="13" />
          </button>
        </span>
      </div>

      <div v-show="!isCollapsed(g.key)" class="sp-items" :data-sg="g.key">
        <div
          v-for="s in g.items"
          :key="s.id"
          class="sp-item"
          :data-si="s.id"
          :data-sg="g.key"
          :class="{
            'drag-source': draggingId === s.id,
            'drop-before': dropItem?.id === s.id && dropItem.before,
            'drop-after': dropItem?.id === s.id && !dropItem.before,
          }"
        >
          <span class="sp-drag" title="拖拽排序" @pointerdown="onItemPointerDown($event, s)">
            <NIcon :component="GripVertical" :size="13" />
          </span>
          <div class="sp-item-info">
            <span class="sp-item-name">{{ s.name }}</span>
            <span class="sp-item-cmd">{{ s.command }}</span>
          </div>
          <span
            v-if="s.kind === 'service'"
            class="sp-dot"
            :class="`sp-dot-${svcStatus(s)}`"
            :title="`状态：${svcStatusText[svcStatus(s)]}`"
          />
          <NIcon
            v-if="stateIcon(s.id)"
            :component="stateIcon(s.id)!.icon"
            :class="['sp-state', stateIcon(s.id)!.cls]"
            :size="14"
          />
          <span class="sp-ops">
            <template v-if="s.kind === 'service'">
              <button
                class="sp-op"
                title="查看状态（systemctl status）"
                :disabled="!store.canRun || store.running"
                @click="runOne(s)"
              >
                <NIcon :component="Eye" :size="13" />
              </button>
              <button
                class="sp-op"
                title="启动"
                :disabled="!store.canRun || store.running"
                @click="svcAction(s, 'start')"
              >
                <NIcon :component="PlayerPlay" :size="13" />
              </button>
              <NPopconfirm
                :disabled="!store.canRun || store.running"
                @positive-click="svcAction(s, 'stop')"
              >
                <template #trigger>
                  <button
                    class="sp-op danger"
                    title="停止"
                    :disabled="!store.canRun || store.running"
                  >
                    <NIcon :component="PlayerStop" :size="13" />
                  </button>
                </template>
                确认停止服务「{{ s.name }}」？
              </NPopconfirm>
              <NPopconfirm
                :disabled="!store.canRun || store.running"
                @positive-click="svcAction(s, 'restart')"
              >
                <template #trigger>
                  <button
                    class="sp-op"
                    title="重启"
                    :disabled="!store.canRun || store.running"
                  >
                    <NIcon :component="Refresh" :size="13" />
                  </button>
                </template>
                确认重启服务「{{ s.name }}」？
              </NPopconfirm>
            </template>
            <button
              v-else
              class="sp-op"
              title="执行"
              :disabled="!store.canRun || store.running"
              @click="runOne(s)"
            >
              <NIcon :component="PlayerPlay" :size="13" />
            </button>
            <button class="sp-op" title="编辑" @click="openEdit(s)">
              <NIcon :component="Pencil" :size="13" />
            </button>
            <button class="sp-op danger" title="删除" @click="removeOne(s)">
              <NIcon :component="Trash" :size="13" />
            </button>
          </span>
        </div>
      </div>
    </div>
    </template>

    <!-- 新增 / 编辑条目（类型由打开方式决定，编辑时 kind 不可改） -->
    <NModal
      :show="showForm"
      preset="card"
      :title="formTitle"
      style="width: 420px"
      @update:show="(v: boolean) => (showForm = v)"
    >
      <div class="sp-form">
        <NInput
          v-model:value="draft.name"
          size="small"
          :placeholder="draft.kind === 'service' ? '名称（留空取服务名）' : '名称（留空取命令前 30 字）'"
        />
        <NInput
          v-if="draft.kind === 'command'"
          v-model:value="draft.command"
          size="small"
          type="textarea"
          :autosize="{ minRows: 2, maxRows: 6 }"
          placeholder="命令（如 docker ps -a）"
        />
        <template v-else>
          <NInput v-model:value="draft.command" size="small" placeholder="服务名（如 nginx）" />
          <NInput
            v-model:value="draft.checkCmd"
            size="small"
            placeholder="检查命令（可选，留空用 systemctl is-active）"
            clearable
          />
        </template>
        <NAutoComplete
          v-model:value="draft.group"
          size="small"
          :options="groupOptions"
          placeholder="所属分组（留空为未分组，可输入新组名）"
          clearable
        />
        <div class="sp-form-actions">
          <NButton size="small" @click="showForm = false">取消</NButton>
          <NButton size="small" type="primary" @click="submitForm">保存</NButton>
        </div>
      </div>
    </NModal>

    <!-- 重命名分组 -->
    <NModal
      :show="showRename"
      preset="card"
      title="重命名分组"
      style="width: 320px"
      @update:show="(v: boolean) => (showRename = v)"
    >
      <div class="sp-form">
        <NAutoComplete
          v-model:value="renameName"
          size="small"
          :options="groupOptions"
          placeholder="分组名（已存在则合并）"
          @keydown.enter="submitRename"
        />
        <div class="sp-form-actions">
          <NButton size="small" @click="showRename = false">取消</NButton>
          <NButton size="small" type="primary" @click="submitRename">确定</NButton>
        </div>
      </div>
    </NModal>
  </section>
</template>

<style scoped>
.snippet-panel {
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.sp-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  margin-bottom: 6px;
}
.sp-tabs {
  flex: 1;
  min-width: 0;
}
.sp-empty {
  padding: 28px 0;
}
.sp-group {
  border-radius: 6px;
}
.sp-group.drag-source,
.sp-item.drag-source {
  opacity: 0.4;
}
.sp-group.drop-before {
  box-shadow: 0 -2px 0 0 var(--primary);
}
.sp-group.drop-after {
  box-shadow: 0 2px 0 0 var(--primary);
}
.sp-group-head {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 5px 4px;
  cursor: pointer;
  border-radius: 6px;
  user-select: none;
}
.sp-group-head:hover {
  background: var(--bg-elevated);
}
.sp-arrow {
  color: var(--text-tertiary);
  transition: transform 0.15s;
  flex-shrink: 0;
}
.sp-arrow.open {
  transform: rotate(90deg);
}
.sp-group-name {
  font-size: 12px;
  font-weight: 600;
  color: var(--text-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.sp-group-count {
  font-size: 11px;
  color: var(--text-tertiary);
  flex-shrink: 0;
}
.sp-items {
  display: flex;
  flex-direction: column;
}
.sp-item {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 4px 4px 4px 18px;
  border-radius: 6px;
}
.sp-item:hover {
  background: var(--bg-elevated);
}
.sp-item.drop-before {
  box-shadow: inset 0 2px 0 0 var(--primary);
}
.sp-item.drop-after {
  box-shadow: inset 0 -2px 0 0 var(--primary);
}
.sp-drag {
  display: flex;
  align-items: center;
  color: var(--text-tertiary);
  cursor: grab;
  flex-shrink: 0;
  touch-action: none;
}
.sp-item-info {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 1px;
}
.sp-item-name {
  font-size: 13px;
  color: var(--text-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.sp-dot {
  flex-shrink: 0;
  width: 8px;
  height: 8px;
  border-radius: 50%;
}
.sp-dot-active {
  background: var(--success, #18a058);
}
.sp-dot-inactive {
  background: var(--danger);
}
.sp-dot-unknown {
  background: var(--text-tertiary);
}
.sp-item-cmd {
  font-size: 11px;
  color: var(--text-tertiary);
  font-family: Consolas, Menlo, monospace;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.sp-state {
  flex-shrink: 0;
}
.sp-state.st-running {
  color: var(--primary);
  animation: sp-spin 0.9s linear infinite;
}
.sp-state.st-success {
  color: var(--success, #18a058);
}
.sp-state.st-failed {
  color: var(--danger);
}
.sp-state.st-pending {
  color: var(--text-tertiary);
}
@keyframes sp-spin {
  to {
    transform: rotate(360deg);
  }
}
.sp-ops {
  display: none;
  align-items: center;
  gap: 2px;
  margin-left: auto;
  flex-shrink: 0;
}
.sp-group-head:hover .sp-ops,
.sp-item:hover .sp-ops {
  display: flex;
}
.sp-op {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 20px;
  height: 20px;
  border: none;
  background: transparent;
  color: var(--text-tertiary);
  cursor: pointer;
  border-radius: 4px;
  padding: 0;
}
.sp-op:hover:not(:disabled) {
  color: var(--text-primary);
  background: var(--bg-panel);
}
.sp-op.danger:hover:not(:disabled) {
  color: var(--danger);
}
.sp-op:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}
.sp-form {
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.sp-form-actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}
</style>

<style>
/* 拖拽 ghost / 拖拽中页面状态：元素挂在 body 下，scoped 样式到不了，需全局定义 */
.snippet-drag-ghost {
  position: fixed;
  z-index: 1000;
  pointer-events: none;
  max-width: 220px;
  padding: 4px 10px;
  font-size: 12px;
  color: var(--text-primary);
  background: var(--bg-elevated);
  border: 1px solid var(--primary);
  border-radius: 4px;
  opacity: 0.92;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
body.snippet-dragging {
  user-select: none;
  cursor: grabbing;
}
</style>
