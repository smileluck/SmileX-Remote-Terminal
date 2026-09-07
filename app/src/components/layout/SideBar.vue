<script setup lang="ts">
/**
 * SideBar - 侧边栏（会话面板）
 *
 * Termius 风卡片化会话列表：
 * - 内容由 ActivityRail「会话 / 远程桌面」入口切换（layout.sidebarTab 持久化，按 kind 过滤）
 * - 顶部 header（标题随页签变化 + 折叠按钮）
 * - 搜索栏右侧「+」按当前页签直接打开对应新建弹窗
 * - 卡片：kind 图标 + 名称 + user@host:port + 相对时间 + 悬浮 编辑/删除
 * - 点击卡片走统一连接流程（已连接弹「切换/新开」，断线原地重连）
 *
 * 分组（extra.group）：
 * - 按分组名忽略大小写聚类（「Wujie」与「WUJIE」视为同组，避免同组分叉）
 * - 拖拽会话卡片到其他分组 → 移动该会话
 * - 拖拽分组头到其他分组 → 合并两组，随后提示重命名
 * - 双击分组头 → 重命名分组
 *
 * 已打开的 Tab 不再在此展示（移至顶部 TabBar）。
 *
 * 数据来源：
 * - profiles store（持久化的 SessionProfile 列表）
 * - tabs store（连接成功后 addTab）
 */
import { computed, nextTick, onMounted, ref, type Component } from 'vue'
import { NButton, NIcon, NPopconfirm, NEmpty, NInput, NModal, NTooltip, useDialog, useMessage } from 'naive-ui'
import { Terminal2, DeviceDesktop, BrandApple, Plus, Pencil, Trash, Search, ChevronRight, LayoutSidebarLeftCollapse } from '@vicons/tabler'
import { useProfilesStore } from '@/stores/profiles'
import { useTabsStore } from '@/stores/tabs'
import { useUiStore } from '@/stores/ui'
import { useLayoutStore } from '@/stores/layout'
import { useConnectFlow } from '@/composables/useConnectFlow'
import { decodeExtra } from '@/types/profile'
import type { SessionProfile } from '@/types/profile'

const profilesStore = useProfilesStore()
const tabsStore = useTabsStore()
const ui = useUiStore()
const layout = useLayoutStore()
const { connect } = useConnectFlow()
const message = useMessage()
const dialog = useDialog()

/** profile.id → 是否有已连接（未断开）的 tab */
function isConnected(profileId: string): boolean {
  return tabsStore.tabs.some(
    (t) => t.profileId === profileId && t.sessionId && !t.disconnected,
  )
}

/** kind → 图标组件（host = macOS，用 BrandApple） */
const kindIcon: Record<string, Component> = {
  ssh: Terminal2,
  rdp: DeviceDesktop,
  host: BrandApple,
}

/** 正在连接的 profile id（禁用按钮防抖） */
const connectingId = ref<string | null>(null)

/** 「+」新建：按当前页签直接打开对应弹窗（SSH 会话 / 远程桌面连接） */
function onAdd() {
  if (layout.sidebarTab === 'ssh') ui.openConnectDialog()
  else ui.openDesktopConnectDialog()
}

/** 搜索关键字（名称 / host / 用户名 / 分组 子串过滤） */
const search = ref('')
/** 已折叠的分组键集合（归一化小写键） */
const collapsedGroups = ref(new Set<string>())

/** profile 的分组原始名（trim 后；无分组返回空串） */
function groupOf(p: SessionProfile): string {
  return decodeExtra(p.extra).group?.trim() || ''
}

/** 分组归一化键：忽略大小写（「Wujie」与「WUJIE」同一组） */
function groupKey(name: string): string {
  return name.toLowerCase()
}

/** 当前页签的 profiles（SSH 会话 tab 只含 ssh；远程桌面 tab 含 rdp/host） */
const tabProfiles = computed(() =>
  layout.sidebarTab === 'ssh'
    ? profilesStore.profiles.filter((p) => p.kind === 'ssh')
    : profilesStore.profiles.filter((p) => p.kind !== 'ssh'),
)

/** 过滤后的 profiles（当前页签 + 搜索关键字） */
const filteredProfiles = computed(() => {
  const kw = search.value.trim().toLowerCase()
  if (!kw) return tabProfiles.value
  return tabProfiles.value.filter(
    (p) =>
      p.name.toLowerCase().includes(kw) ||
      p.host.toLowerCase().includes(kw) ||
      p.username.toLowerCase().includes(kw) ||
      groupOf(p).toLowerCase().includes(kw),
  )
})

/** 侧栏分组桶 */
interface GroupBucket {
  /** 归一化键（小写；'' = 未分组） */
  key: string
  /** 显示名（组内首个出现的原始写法） */
  name: string
  profiles: SessionProfile[]
}

/** 按 group 忽略大小写聚类（有名分组按名排序，「未分组」最后） */
const groupedProfiles = computed<GroupBucket[]>(() => {
  const map = new Map<string, GroupBucket>()
  for (const p of filteredProfiles.value) {
    const raw = groupOf(p)
    const key = groupKey(raw)
    let bucket = map.get(key)
    if (!bucket) {
      bucket = { key, name: raw, profiles: [] }
      map.set(key, bucket)
    }
    bucket.profiles.push(p)
  }
  const named = [...map.values()]
    .filter((b) => b.key)
    .sort((a, b) => a.name.localeCompare(b.name, 'zh'))
  const none = map.get('')
  if (none) named.push(none)
  return named
})

/** 搜索时自动展开所有分组 */
const isGroupCollapsed = (g: GroupBucket) => !search.value.trim() && collapsedGroups.value.has(g.key)

function toggleGroup(g: GroupBucket) {
  const s = new Set(collapsedGroups.value)
  if (s.has(g.key)) s.delete(g.key)
  else s.add(g.key)
  collapsedGroups.value = s
}

// ---------------------------------------------------------------------------
// 拖拽：会话卡片 → 分组（移动）；分组头 → 分组（合并 + 提示重命名）
// Pointer Events 自实现：Tauri dragDropEnabled 会拦截 HTML5 drop（FilePanel
// 的 OS 文件拖入依赖该行为），原生 DnD 在页面内不可用。
// ---------------------------------------------------------------------------

/** 正在拖拽的会话 id / 分组键（源高亮用） */
const dragProfileId = ref<string | null>(null)
const dragGroupKey = ref<string | null>(null)
/** 当前拖拽悬停的目标分组键（drop-target 高亮用；'' = 未分组） */
const dropGroupKey = ref<string | null>(null)

/** 拖拽判定阈值（px）：位移内视为点击，保证单击连接/双击重命名不受影响 */
const DRAG_THRESHOLD = 6

/** 进行中的指针拖拽（null = 无；started 前不拦截任何默认行为） */
interface PointerDrag {
  kind: 'session' | 'group'
  /** session：会话 id；group：分组键 */
  id: string
  /** ghost 显示文本 */
  label: string
  startX: number
  startY: number
  started: boolean
  ghost: HTMLElement | null
}
let pointerDrag: PointerDrag | null = null

function onCardPointerDown(e: PointerEvent, p: SessionProfile) {
  if (e.button !== 0) return
  beginPointerDrag(e, { kind: 'session', id: p.id, label: p.name })
}

function onGroupPointerDown(e: PointerEvent, g: GroupBucket) {
  // 「未分组」不可作为整体拖走
  if (e.button !== 0 || !g.key) return
  beginPointerDrag(e, { kind: 'group', id: g.key, label: g.name })
}

function beginPointerDrag(e: PointerEvent, info: Pick<PointerDrag, 'kind' | 'id' | 'label'>) {
  e.preventDefault() // 阻止文本选中；不影响后续 click/dblclick
  pointerDrag = { ...info, startX: e.clientX, startY: e.clientY, started: false, ghost: null }
  window.addEventListener('pointermove', onDragPointerMove)
  window.addEventListener('pointerup', onDragPointerUp)
  window.addEventListener('pointercancel', onDragPointerUp)
}

function createGhost(label: string): HTMLElement {
  const el = document.createElement('div')
  el.className = 'sidebar-drag-ghost'
  el.textContent = label
  document.body.appendChild(el)
  return el
}

function moveGhost(ghost: HTMLElement | null, x: number, y: number) {
  if (!ghost) return
  ghost.style.left = `${x + 12}px`
  ghost.style.top = `${y + 12}px`
}

/** 指针位置命中的分组键（依赖分组头/分组体上的 data-group-key；未命中返回 null） */
function hitGroupKey(x: number, y: number): string | null {
  const el = document.elementFromPoint(x, y)
  const host = el?.closest('[data-group-key]') as HTMLElement | null
  return host?.dataset.groupKey ?? null
}

function onDragPointerMove(e: PointerEvent) {
  const d = pointerDrag
  if (!d) return
  if (!d.started) {
    if (Math.hypot(e.clientX - d.startX, e.clientY - d.startY) < DRAG_THRESHOLD) return
    d.started = true
    d.ghost = createGhost(d.label)
    document.body.classList.add('sidebar-dragging')
    if (d.kind === 'session') dragProfileId.value = d.id
    else dragGroupKey.value = d.id
  }
  moveGhost(d.ghost, e.clientX, e.clientY)
  const key = hitGroupKey(e.clientX, e.clientY)
  // 分组不能拖到自己身上
  dropGroupKey.value = d.kind === 'group' && key === d.id ? null : key
}

function cleanupPointerDrag() {
  window.removeEventListener('pointermove', onDragPointerMove)
  window.removeEventListener('pointerup', onDragPointerUp)
  window.removeEventListener('pointercancel', onDragPointerUp)
  pointerDrag?.ghost?.remove()
  document.body.classList.remove('sidebar-dragging')
  pointerDrag = null
  dragProfileId.value = null
  dragGroupKey.value = null
  dropGroupKey.value = null
}

function onDragPointerUp(e: PointerEvent) {
  const d = pointerDrag
  const key = d?.started ? hitGroupKey(e.clientX, e.clientY) : null
  cleanupPointerDrag()
  // 未过阈值：视为点击，click/dblclick 自然触发
  if (!d || !d.started || key === null) return
  const target = groupedProfiles.value.find((b) => b.key === key)
  if (!target) return

  if (d.kind === 'session') {
    // 已在目标分组则不动作（避免无意义的写库与提示）
    const p = profilesStore.findById(d.id)
    if (p && groupKey(groupOf(p)) === target.key) return
    const targetName = target.name || '未分组'
    dialog.warning({
      title: '移动会话',
      content: `将「${p?.name ?? d.id}」移动到「${targetName}」？`,
      positiveText: '移动',
      negativeText: '取消',
      onPositiveClick: () => moveSessions([d.id], target),
    })
  } else if (d.id !== target.key) {
    const sourceName = groupedProfiles.value.find((b) => b.key === d.id)?.name || d.id
    const targetName = target.name || '未分组'
    dialog.warning({
      title: '合并分组',
      content: `将分组「${sourceName}」的全部会话合并到「${targetName}」？`,
      positiveText: '合并',
      negativeText: '取消',
      onPositiveClick: () => mergeGroups(d.id, target),
    })
  }
}

/** 把会话移动到目标分组（目标为未分组则移出） */
async function moveSessions(ids: string[], target: GroupBucket) {
  try {
    await profilesStore.setGroup(ids, target.name || undefined)
    message.success(target.key ? `已移动到「${target.name}」` : '已移出分组')
  } catch (e) {
    message.error(`移动失败：${e}`)
  }
}

/** 把源分组的全部会话并入目标分组；并入命名分组后提示重命名 */
async function mergeGroups(sourceKey: string, target: GroupBucket) {
  const ids = profilesStore.profiles
    .filter((p) => groupKey(groupOf(p)) === sourceKey)
    .map((p) => p.id)
  if (!ids.length) return
  try {
    await profilesStore.setGroup(ids, target.name || undefined)
    message.success(`已合并 ${ids.length} 个会话到「${target.name || '未分组'}」`)
    if (target.key) openRename(target.key)
  } catch (e) {
    message.error(`合并分组失败：${e}`)
  }
}

// ---------------------------------------------------------------------------
// 分组重命名（双击分组头 / 合并后的提示）
// ---------------------------------------------------------------------------

const renameVisible = ref(false)
/** 待重命名分组的归一化键 */
const renameKey = ref('')
const renameValue = ref('')
const renameBusy = ref(false)
const renameInputRef = ref<InstanceType<typeof NInput> | null>(null)

function openRename(key: string) {
  const bucket = groupedProfiles.value.find((g) => g.key === key)
  renameKey.value = key
  renameValue.value = bucket?.name ?? ''
  renameVisible.value = true
  nextTick(() => renameInputRef.value?.focus())
}

/** 确认重命名：把该组全部会话改到新分组名（返回 false 时弹窗不关闭） */
async function confirmRename(): Promise<boolean> {
  const name = renameValue.value.trim()
  if (!name) {
    message.warning('分组名不能为空')
    return false
  }
  const ids = profilesStore.profiles
    .filter((p) => groupKey(groupOf(p)) === renameKey.value)
    .map((p) => p.id)
  renameBusy.value = true
  try {
    await profilesStore.setGroup(ids, name)
    renameVisible.value = false
    message.success(`分组已重命名为「${name}」`)
    return true
  } catch (e) {
    message.error(`重命名失败：${e}`)
    return false
  } finally {
    renameBusy.value = false
  }
}

/** 相对时间格式化（last_used_at → 「刚刚 / x 分钟前 / …」） */
function formatRelativeTime(ts: number): string {
  if (!ts) return '从未连接'
  const diff = Math.floor(Date.now() / 1000) - ts
  if (diff < 60) return '刚刚'
  if (diff < 3600) return `${Math.floor(diff / 60)} 分钟前`
  if (diff < 86400) return `${Math.floor(diff / 3600)} 小时前`
  return `${Math.floor(diff / 86400)} 天前`
}

/** 删除会话配置（由 NPopconfirm 确认后触发） */
async function onDelete(profile: SessionProfile) {
  try {
    await profilesStore.remove(profile.id)
    message.success(`已删除「${profile.name}」`)
  } catch (e) {
    message.error(String(e))
  }
}

/** 点击会话卡片发起连接（统一流程：已连接弹「切换/新开」，断线原地重连） */
async function onConnect(profile: SessionProfile) {
  connectingId.value = profile.id
  try {
    await connect(profile)
  } catch (e) {
    message.error(`连接失败：${e}`)
  } finally {
    connectingId.value = null
  }
}

/** 编辑：按 kind 打开对应弹窗（SSH 会话 / 远程桌面） */
function onEdit(p: SessionProfile) {
  if (p.kind === 'ssh') ui.openConnectDialog(p.id)
  else ui.openDesktopConnectDialog(p.id)
}

onMounted(() => {
  profilesStore.loadAll()
})
</script>

<template>
  <aside class="side-bar">
    <header class="section-header">
      <span class="section-title">{{ layout.sidebarTab === 'ssh' ? '会话' : '远程桌面' }}</span>
      <NTooltip placement="bottom">
        <template #trigger>
          <NButton quaternary size="tiny" circle title="折叠会话栏" @click="layout.toggleSidebar()">
            <NIcon :component="LayoutSidebarLeftCollapse" />
          </NButton>
        </template>
        折叠会话栏（⌘B）
      </NTooltip>
    </header>

    <div class="search-slot">
      <NInput v-model:value="search" size="small" placeholder="搜索会话 / 主机 / 分组" clearable>
        <template #prefix><NIcon :component="Search" /></template>
      </NInput>
      <NButton
        quaternary
        size="small"
        circle
        :title="layout.sidebarTab === 'ssh' ? '新建 SSH 会话' : '新建远程桌面连接'"
        @click="onAdd"
      >
        <NIcon :component="Plus" />
      </NButton>
    </div>

    <div class="list-scroll">
      <!-- 空状态 -->
      <div v-if="tabProfiles.length === 0" class="empty">
        <NEmpty
          size="small"
          :description="layout.sidebarTab === 'ssh' ? '暂无会话' : '暂无远程桌面'"
        >
          <template #extra>
            <span class="empty-hint">点击搜索框右侧 + 新建</span>
          </template>
        </NEmpty>
      </div>

      <!-- 搜索无结果 -->
      <div v-else-if="filteredProfiles.length === 0" class="empty">
        <NEmpty size="small" description="无匹配会话" />
      </div>

      <!-- 分组卡片列表 -->
      <div v-else class="card-list">
        <div
          v-for="g in groupedProfiles"
          :key="g.key || '__none__'"
          class="group"
          :class="{ 'drop-target': dropGroupKey === g.key, 'drag-source': dragGroupKey === g.key }"
        >
          <!-- 分组头（仅多组或已命名时显示）：点击折叠、双击重命名、可拖拽合并 -->
          <div
            v-if="groupedProfiles.length > 1 || g.name"
            class="group-header"
            :data-group-key="g.key"
            :title="g.key ? '拖拽会话到此移动；拖拽分组头合并；双击重命名' : '拖拽会话到此移出分组'"
            @click="toggleGroup(g)"
            @dblclick="g.key && openRename(g.key)"
            @pointerdown="onGroupPointerDown($event, g)"
          >
            <NIcon :component="ChevronRight" class="chevron" :class="{ open: !isGroupCollapsed(g) }" />
            <span class="group-name">{{ g.name || '未分组' }}</span>
            <span class="group-count">{{ g.profiles.length }}</span>
          </div>

          <div
            v-show="!isGroupCollapsed(g)"
            class="group-body"
            :data-group-key="g.key"
          >
            <div v-for="p in g.profiles" :key="p.id" class="card-slot">
              <div
                class="profile-card"
                :class="{ dragging: dragProfileId === p.id }"
                :title="`${p.username}@${p.host}:${p.port}`"
                @click="onConnect(p)"
                @pointerdown="onCardPointerDown($event, p)"
              >
                <NIcon
                  :component="kindIcon[p.kind] || Terminal2"
                  class="card-icon"
                  :data-kind="p.kind"
                />
                <div class="card-main">
                  <div class="card-title">
                    <span
                      class="status-dot"
                      :class="isConnected(p.id) ? 'on' : connectingId === p.id ? 'pending' : 'off'"
                    />
                    {{ p.name }}
                  </div>
                  <div class="card-meta">{{ p.username }}@{{ p.host }}:{{ p.port }}</div>
                  <div class="card-sub">
                    <span v-if="connectingId === p.id" class="connecting">● 连接中…</span>
                    <span v-else class="time">{{ formatRelativeTime(p.last_used_at) }}</span>
                  </div>
                </div>
                <div class="card-actions">
                  <NButton text size="tiny" title="编辑" @click.stop="onEdit(p)">
                    <NIcon :component="Pencil" />
                  </NButton>
                  <NPopconfirm @positive-click="onDelete(p)">
                    <template #trigger>
                      <NButton
                        text
                        size="tiny"
                        class="danger"
                        title="删除"
                        @click.stop
                      >
                        <NIcon :component="Trash" />
                      </NButton>
                    </template>
                    确定删除会话「{{ p.name }}」吗？
                  </NPopconfirm>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>

    <!-- 分组重命名弹窗（双击分组头 / 合并分组后弹出） -->
    <NModal
      v-model:show="renameVisible"
      preset="dialog"
      title="重命名分组"
      positive-text="确定"
      negative-text="取消"
      :loading="renameBusy"
      @positive-click="confirmRename"
    >
      <NInput
        ref="renameInputRef"
        v-model:value="renameValue"
        placeholder="输入新的分组名称"
        :disabled="renameBusy"
        @keydown.enter="confirmRename"
      />
    </NModal>
  </aside>
</template>

<style scoped>
.side-bar {
  width: 280px;
  flex-shrink: 0;
  background: var(--bg-sidebar);
  border-right: 1px solid var(--border-color);
  display: flex;
  flex-direction: column;
  overflow: hidden;
}
.section-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 10px 14px;
  border-bottom: 1px solid var(--border-color);
  flex-shrink: 0;
}
.section-title {
  font-size: 12px;
  font-weight: 600;
  color: var(--text-secondary);
  text-transform: uppercase;
  letter-spacing: 1px;
}
.list-scroll {
  flex: 1;
  overflow-y: auto;
  padding: 8px;
}
.search-slot {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 8px 10px;
  border-bottom: 1px solid var(--border-color);
  flex-shrink: 0;
}
.search-slot :deep(.n-input) {
  flex: 1;
  min-width: 0;
}
.group {
  display: flex;
  flex-direction: column;
  border-radius: var(--radius-sm, 4px);
  transition: background 0.12s, box-shadow 0.12s;
}
.group + .group {
  margin-top: 8px;
}
/* 拖拽悬停目标：整组高亮描边 */
.group.drop-target {
  background: var(--primary-bg);
  box-shadow: inset 0 0 0 1px var(--primary-border);
}
/* 正在被拖拽的分组：半透明提示 */
.group.drag-source {
  opacity: 0.45;
}
/* 正在被拖拽的会话卡片 */
.profile-card.dragging {
  opacity: 0.45;
}
.group-header {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 4px 6px;
  cursor: pointer;
  user-select: none;
  border-radius: var(--radius-sm, 4px);
  color: var(--text-secondary);
}
.group-header:hover {
  background: var(--bg-elevated);
}
.chevron {
  font-size: 13px;
  transition: transform 0.12s;
}
.chevron.open {
  transform: rotate(90deg);
}
.group-name {
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.5px;
  text-transform: uppercase;
}
.group-count {
  font-size: 10px;
  color: var(--text-tertiary);
  margin-left: 2px;
}
.group-body {
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding-left: 6px;
}
.empty {
  padding: 32px 12px;
  display: flex;
  justify-content: center;
}
.empty-hint {
  font-size: 12px;
  color: var(--text-tertiary);
}
.card-list,
.card-slot {
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.profile-card {
  display: flex;
  align-items: flex-start;
  gap: 10px;
  padding: 10px;
  border-radius: var(--radius-md);
  cursor: pointer;
  position: relative;
  border: 1px solid transparent;
  transition: background 0.12s, border-color 0.12s;
}
.profile-card:hover {
  background: var(--bg-elevated);
  border-color: var(--border-color);
}
.card-icon {
  font-size: 18px;
  margin-top: 2px;
  flex-shrink: 0;
}
.card-icon[data-kind='ssh'] { color: var(--kind-ssh-fg); }
.card-icon[data-kind='rdp'] { color: var(--kind-rdp-fg); }
.card-icon[data-kind='host'] { color: var(--kind-host-fg); }
.card-main {
  flex: 1;
  min-width: 0;
}
.status-dot {
  display: inline-block;
  width: 6px;
  height: 6px;
  border-radius: 50%;
  margin-right: 5px;
  vertical-align: middle;
}
.status-dot.on {
  background: var(--success, #34d399);
}
.status-dot.pending {
  background: var(--warning, #fbbf24);
}
.status-dot.off {
  background: transparent;
}
.card-title {
  font-size: 13px;
  font-weight: 500;
  color: var(--text-primary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.card-meta {
  font-size: 11px;
  color: var(--text-secondary);
  margin-top: 2px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-family: var(--font-mono, ui-monospace, monospace);
}
.card-sub {
  font-size: 11px;
  margin-top: 2px;
}
.card-sub .time {
  color: var(--text-tertiary);
}
.connecting {
  color: var(--warning);
}
.card-actions {
  display: flex;
  gap: 2px;
  opacity: 0;
  transition: opacity 0.12s;
}
.profile-card:hover .card-actions {
  opacity: 1;
}
.card-actions :deep(.n-button) {
  --n-text-color: var(--text-tertiary);
  font-size: 14px;
}
.card-actions :deep(.n-button:hover) {
  --n-text-color: var(--text-primary);
}
.card-actions .danger:hover {
  --n-text-color: var(--danger) !important;
}
</style>

<style>
/* 拖拽 ghost / 拖拽中页面状态：元素挂在 body 下，scoped 样式到不了，需全局定义 */
.sidebar-drag-ghost {
  position: fixed;
  z-index: 1000;
  pointer-events: none;
  max-width: 220px;
  padding: 4px 10px;
  font-size: 12px;
  color: var(--text-primary);
  background: var(--bg-elevated);
  border: 1px solid var(--primary);
  border-radius: var(--radius-sm, 4px);
  opacity: 0.92;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
body.sidebar-dragging {
  user-select: none;
  cursor: grabbing;
}
</style>
