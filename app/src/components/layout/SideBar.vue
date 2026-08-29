<script setup lang="ts">
/**
 * SideBar - 侧边栏（会话面板）
 *
 * Termius 风卡片化会话列表：
 * - 顶部 header（「会话」标题 + 新建按钮）
 * - 卡片：kind 图标 + 名称 + user@host:port + 相对时间 + 悬浮 编辑/删除
 * - 点击卡片发起连接；编辑/新建 inline 展开 ConnectForm
 *
 * 已打开的 Tab 不再在此展示（移至顶部 TabBar）。
 *
 * 数据来源：
 * - profiles store（持久化的 SessionProfile 列表）
 * - tabs store（连接成功后 addTab）
 */
import { computed, onMounted, ref, type Component } from 'vue'
import { NButton, NIcon, NPopconfirm, NEmpty, NInput, useMessage } from 'naive-ui'
import { Terminal2, DeviceDesktop, BrandApple, Plus, Pencil, Trash, Search, ChevronRight } from '@vicons/tabler'
import { useProfilesStore } from '@/stores/profiles'
import { useTabsStore } from '@/stores/tabs'
import { useMonitorStore } from '@/stores/monitor'
import { connectProfile } from '@/composables/useSshConnect'
import * as profileService from '@/services/profile'
import { decodeExtra } from '@/types/profile'
import type { SessionProfile } from '@/types/profile'
import ConnectForm from '@/components/common/ConnectForm.vue'

const profilesStore = useProfilesStore()
const tabsStore = useTabsStore()
const monitorStore = useMonitorStore()
const message = useMessage()

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

/** 是否正在显示编辑表单（profileId 非 null 表示进入编辑/新建） */
const editingProfileId = ref<string | null>(null)
/** 是否处于「新建」模式 */
const creating = ref(false)
/** 正在连接的 profile id（禁用按钮防抖） */
const connectingId = ref<string | null>(null)

/** 搜索关键字（名称 / host / 用户名 / 分组 子串过滤） */
const search = ref('')
/** 已折叠的分组名集合 */
const collapsedGroups = ref(new Set<string>())

/** 过滤后的 profiles */
const filteredProfiles = computed(() => {
  const kw = search.value.trim().toLowerCase()
  if (!kw) return profilesStore.profiles
  return profilesStore.profiles.filter(
    (p) =>
      p.name.toLowerCase().includes(kw) ||
      p.host.toLowerCase().includes(kw) ||
      p.username.toLowerCase().includes(kw) ||
      (decodeExtra(p.extra).group || '').toLowerCase().includes(kw),
  )
})

/** 按 group 聚类（有名分组按名排序，「未分组」最后） */
const groupedProfiles = computed(() => {
  const map = new Map<string, SessionProfile[]>()
  for (const p of filteredProfiles.value) {
    const g = decodeExtra(p.extra).group || ''
    if (!map.has(g)) map.set(g, [])
    map.get(g)!.push(p)
  }
  const names = [...map.keys()].filter((n) => n).sort((a, b) => a.localeCompare(b, 'zh'))
  if (map.has('')) names.push('')
  return names.map((n) => ({ name: n, profiles: map.get(n)! }))
})

/** 搜索时自动展开所有分组 */
const isGroupCollapsed = (name: string) => !search.value.trim() && collapsedGroups.value.has(name)

function toggleGroup(name: string) {
  const s = new Set(collapsedGroups.value)
  if (s.has(name)) s.delete(name)
  else s.add(name)
  collapsedGroups.value = s
}

/** 进入新建模式 */
function startCreate() {
  creating.value = true
  editingProfileId.value = null
}

/** 进入编辑模式 */
function startEdit(id: string) {
  creating.value = false
  editingProfileId.value = id
}

/** 关闭表单 */
function closeForm() {
  creating.value = false
  editingProfileId.value = null
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

/** 点击会话卡片发起连接 */
async function onConnect(profile: SessionProfile) {
  connectingId.value = profile.id
  try {
    const sessionId = await connectProfile(profile)

    tabsStore.addTab(profile.kind, profile.name, sessionId, profile.id)
    // 自动启动监控采样并设为监控目标
    monitorStore.setActive(sessionId)
    // 更新最近使用时间
    await profileService.touch(profile.id).catch(() => {
      /* 非关键失败：忽略 */
    })
    await profilesStore.loadAll()
  } catch (e) {
    message.error(`连接失败：${e}`)
  } finally {
    connectingId.value = null
  }
}

onMounted(() => {
  profilesStore.loadAll()
})
</script>

<template>
  <aside class="side-bar">
    <header class="section-header">
      <span class="section-title">会话</span>
      <NButton quaternary size="tiny" circle title="新建会话" @click="startCreate">
        <NIcon :component="Plus" />
      </NButton>
    </header>

    <div class="search-slot">
      <NInput v-model:value="search" size="small" placeholder="搜索会话 / 主机 / 分组" clearable>
        <template #prefix><NIcon :component="Search" /></template>
      </NInput>
    </div>

    <div class="list-scroll">
      <!-- 新建表单 -->
      <div v-if="creating" class="form-slot">
        <ConnectForm @close="closeForm" />
      </div>

      <!-- 空状态 -->
      <div v-else-if="profilesStore.profiles.length === 0" class="empty">
        <NEmpty size="small" description="暂无会话">
          <template #extra>
            <span class="empty-hint">点击上方 + 新建</span>
          </template>
        </NEmpty>
      </div>

      <!-- 搜索无结果 -->
      <div v-else-if="filteredProfiles.length === 0" class="empty">
        <NEmpty size="small" description="无匹配会话" />
      </div>

      <!-- 分组卡片列表 -->
      <div v-else class="card-list">
        <div v-for="g in groupedProfiles" :key="g.name || '__none__'" class="group">
          <!-- 分组头（仅多组或已命名时显示） -->
          <div
            v-if="groupedProfiles.length > 1 || g.name"
            class="group-header"
            @click="toggleGroup(g.name)"
          >
            <NIcon :component="ChevronRight" class="chevron" :class="{ open: !isGroupCollapsed(g.name) }" />
            <span class="group-name">{{ g.name || '未分组' }}</span>
            <span class="group-count">{{ g.profiles.length }}</span>
          </div>

          <div v-show="!isGroupCollapsed(g.name)" class="group-body">
            <div v-for="p in g.profiles" :key="p.id" class="card-slot">
          <!-- 卡片（默认） -->
          <div
            v-if="editingProfileId !== p.id"
            class="profile-card"
            :title="`${p.username}@${p.host}:${p.port}`"
            @click="onConnect(p)"
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
              <NButton text size="tiny" title="编辑" @click.stop="startEdit(p.id)">
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

          <!-- 编辑表单（inline） -->
          <div v-else class="form-slot">
            <ConnectForm :profile-id="p.id" @close="closeForm" />
          </div>
        </div>
          </div>
        </div>
      </div>
    </div>
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
  padding: 8px 10px;
  border-bottom: 1px solid var(--border-color);
  flex-shrink: 0;
}
.group {
  display: flex;
  flex-direction: column;
}
.group + .group {
  margin-top: 8px;
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
.form-slot {
  padding: 4px 0;
}
</style>
