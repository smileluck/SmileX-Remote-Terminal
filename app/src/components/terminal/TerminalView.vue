<script setup lang="ts">
/**
 * TerminalView - 单会话 SSH 终端视图
 *
 * - tab 已带 sessionId（统一连接入口创建）：由 PaneTerminal 渲染 xterm 并绑定会话；
 *   视图按 tab 常驻（MainContent 终端池），跨 group 拖动不重建、不丢终端内容
 * - 会话意外断开（disconnected）：显示断开遮罩 + 重新连接按钮（原地重连）
 * - 右侧工具栏（流式竖排，不悬浮）：分屏单入口（克隆会话到新 group，
 *   点击弹二级菜单选方向）+ 面板入口（文件管理 / 监控 / Agent / 告警 等），
 *   面板互斥切换（同一时间只开一个，再点同一个收起）
 * - 点击视图聚焦所属 group（右栏面板跟随该 group 的激活 tab）
 * - ⌘D / ⌘⇧D 克隆会话到右/下方新 group；⌘⇧W 关闭本 group 全部 tab；
 *   ⌘⌥←→↑↓ 切换聚焦 group
 */
import { ref, computed, onMounted, onUnmounted, h, type Component } from 'vue'
import { NButton, NDropdown, NIcon, NTooltip, useMessage } from 'naive-ui'
import {
  Refresh,
  Folder,
  ArrowsSplit2,
  LayoutRows,
  Robot,
  ChartAreaLine,
  Bell,
  Bookmarks,
  BrandDocker,
  CalendarTime,
  Package,
  ArrowsRightLeft,
} from '@vicons/tabler'
import * as sessionService from '@/services/session'
import { useConnectFlow } from '@/composables/useConnectFlow'
import { shellQuote } from '@/utils/shell'
import { useProfilesStore } from '@/stores/profiles'
import { useLayoutStore } from '@/stores/layout'
import { useTabsStore } from '@/stores/tabs'
import FilePanel from '@/components/sftp/FilePanel.vue'
import PaneTerminal from './PaneTerminal.vue'
import type { TabItem } from '@/types/session'

const props = defineProps<{ tab: TabItem }>()
const profiles = useProfilesStore()
const layout = useLayoutStore()
const tabs = useTabsStore()
const message = useMessage()
const { reconnectInTab } = useConnectFlow()

/** 右栏面板入口（Agent / 监控 / 告警 / 常用记录 / Docker / 定时任务 / 环境管理 / 端口转发）：按钮高亮条件与点击切换 */
const panelEntries = [
  { key: 'monitor', label: '监控看板（⌘M）', icon: ChartAreaLine },
  { key: 'agent', label: 'AI 运维助手（⌘J）', icon: Robot },
  { key: 'alerts', label: '告警规则', icon: Bell },
  { key: 'snippets', label: '常用记录', icon: Bookmarks },
  { key: 'docker', label: 'Docker 管理', icon: BrandDocker },
  { key: 'crontab', label: '定时任务', icon: CalendarTime },
  { key: 'env', label: '环境管理', icon: Package },
  { key: 'tunnel', label: '端口转发', icon: ArrowsRightLeft },
] as const

/** 分屏下拉选项（二级选择分割方向） */
function renderIcon(icon: Component, style?: string) {
  return () => h(NIcon, null, { default: () => h(icon, style ? { style } : undefined) })
}
const splitOptions = [
  { label: '向右分屏', key: 'row', icon: renderIcon(ArrowsSplit2, 'transform: rotate(90deg)') },
  { label: '向下分屏', key: 'column', icon: renderIcon(LayoutRows) },
]

/** 重连中 */
const reconnecting = ref(false)

/** 重连目标档案（tab 带 profileId 时可用） */
const profile = computed(() =>
  props.tab.profileId ? profiles.profiles.find((p) => p.id === props.tab.profileId) : null,
)

/** 点击视图：聚焦所属 group（右栏面板 / 快捷键以聚焦 group 的激活 tab 为准） */
function onViewMouseDown() {
  tabs.setActive(props.tab.id)
}

/**
 * 克隆当前会话为独立新会话，放到本 group 右/下方的新 group（Xshell 式分屏）；
 * initialPath 时新会话就绪后 cd 过去（SFTP「在此处打开终端」用）
 */
const splitting = ref(false)
async function cloneToGroup(dir: 'row' | 'column', initialPath?: string) {
  if (splitting.value) return
  const sid = props.tab.sessionId
  if (!sid || sid === 'connected') return
  const srcGroup = tabs.groupOf(props.tab.id)
  if (!srcGroup) return
  if (tabs.groups.length >= 4) {
    message.warning('已达最大分屏数（4）')
    return
  }
  splitting.value = true
  try {
    const newSid = await sessionService.clone(sid, 80, 24)
    const newTab = tabs.addTab('ssh', props.tab.title, newSid, props.tab.profileId)
    tabs.splitWithTab(newTab.id, srcGroup.id, dir, false)
    // shell 就绪无信号，延时后发送 cd
    if (initialPath) {
      setTimeout(() => {
        sessionService
          .input(newSid, Array.from(new TextEncoder().encode(`cd ${shellQuote(initialPath)}\r`)))
          .catch(() => {})
      }, 500)
    }
  } catch (e) {
    message.error(`克隆会话失败：${e}`)
  } finally {
    splitting.value = false
  }
}

/** 关闭本 group 全部 tab（group 随之折叠） */
function closeGroup() {
  const g = tabs.groupOf(props.tab.id)
  if (!g) return
  for (const t of [...g.tabs]) tabs.closeTab(t.id)
}

/**
 * 快捷键（window 监听 + 全局激活门控：多 TerminalView 实例并存时防串扰）：
 * ⌘D 克隆到右侧新 group / ⌘⇧D 克隆到下方新 group / ⌘⇧W 关闭本 group / ⌘⌥←→↑↓ 切换聚焦 group
 */
function onKeydown(e: KeyboardEvent) {
  if (tabs.activeId !== props.tab.id) return
  if (!e.metaKey) return
  const key = e.key.toLowerCase()
  if (key === 'd') {
    e.preventDefault()
    void cloneToGroup(e.shiftKey ? 'column' : 'row')
  } else if (key === 'w' && e.shiftKey) {
    // App.vue 的全局 ⌘W 已跳过 Shift 组合（见 onKeydown）
    e.preventDefault()
    closeGroup()
  } else if (e.altKey && (key === 'arrowleft' || key === 'arrowup')) {
    e.preventDefault()
    tabs.cycleGroup(-1)
  } else if (e.altKey && (key === 'arrowright' || key === 'arrowdown')) {
    e.preventDefault()
    tabs.cycleGroup(1)
  }
}

onMounted(() => window.addEventListener('keydown', onKeydown))
onUnmounted(() => window.removeEventListener('keydown', onKeydown))

/** 重新连接（基于档案，复用当前 tab；新 sessionId 由 PaneTerminal watch 重绑） */
async function handleReconnect() {
  if (!profile.value) {
    message.warning('该会话无关联配置，请从左侧列表重新连接')
    return
  }
  reconnecting.value = true
  try {
    await reconnectInTab(props.tab, profile.value)
    message.success(`已重新连接「${profile.value.name}」`)
  } catch (e) {
    message.error(`重连失败：${e}`)
  } finally {
    reconnecting.value = false
  }
}
</script>

<template>
  <div class="terminal-view" @mousedown="onViewMouseDown">
    <div class="terminal-main">
      <div class="term-area">
        <PaneTerminal :session-id="tab.sessionId ?? null" />
      </div>
      <!-- 工具栏：常规流式竖排（终端区与文件面板之间），不悬浮遮挡任何内容 -->
      <div class="view-tools">
          <!-- 分屏：单入口，二级菜单选择分割方向 -->
          <NDropdown
            v-if="tabs.groups.length < 4 && tab.sessionId"
            trigger="click"
            placement="bottom-end"
            :options="splitOptions"
            @select="(key: string | number) => cloneToGroup(key as 'row' | 'column')"
          >
            <NButton quaternary circle size="small" :loading="splitting">
              <NIcon :component="ArrowsSplit2" />
            </NButton>
          </NDropdown>
          <!-- SFTP 文件面板 -->
          <NTooltip v-if="tab.sessionId" placement="left">
            <template #trigger>
              <NButton
                quaternary
                circle
                size="small"
                :type="layout.filesVisible ? 'primary' : 'default'"
                @click="layout.toggleFiles()"
              >
                <NIcon :component="Folder" />
              </NButton>
            </template>
            文件管理（SFTP）
          </NTooltip>
          <!-- 面板入口：文件管理 / 监控 / Agent / 告警，互斥切换 -->
          <NTooltip v-for="entry in panelEntries" :key="entry.key" placement="left">
            <template #trigger>
              <NButton
                quaternary
                circle
                size="small"
                :type="layout.monitorVisible && layout.rightTab === entry.key ? 'primary' : 'default'"
                @click="layout.toggleRightPanel(entry.key)"
              >
                <NIcon :component="entry.icon" />
              </NButton>
            </template>
            {{ entry.label }}
          </NTooltip>
      </div>
      <!-- SFTP 文件面板 -->
      <FilePanel
        v-if="layout.filesVisible && tab.sessionId && !tab.disconnected"
        :session-id="tab.sessionId"
        :nav-path="layout.filesNavPath"
        @open-split-at="(p: string) => cloneToGroup('row', p)"
      />
    </div>
    <!-- 断开遮罩 -->
    <div v-if="tab.disconnected" class="disconnect-overlay">
      <div class="disconnect-card">
        <p class="disconnect-text">会话已断开</p>
        <NButton
          type="primary"
          size="small"
          :loading="reconnecting"
          @click="handleReconnect"
        >
          <template #icon><NIcon :component="Refresh" /></template>
          重新连接
        </NButton>
      </div>
    </div>
  </div>
</template>

<style scoped>
.terminal-view {
  position: relative;
  display: flex;
  flex-direction: column;
  width: 100%;
  height: 100%;
  background: var(--bg-app);
}
.terminal-main {
  position: relative;
  display: flex;
  flex: 1;
  min-height: 0;
}
.term-area {
  flex: 1;
  min-width: 0;
  min-height: 0;
  display: flex;
}
.term-area :deep(.pane) {
  flex: 1;
  min-width: 0;
  min-height: 0;
}
.view-tools {
  flex-shrink: 0;
  width: 38px;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 4px;
  padding: 8px 0;
  border-left: 1px solid var(--border-color);
  background: var(--bg-app);
}
.disconnect-overlay {
  position: absolute;
  inset: 0;
  background: rgba(15, 20, 25, 0.72);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 10;
}
.disconnect-card {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 10px;
  padding: 20px 28px;
  background: var(--bg-panel, #161c24);
  border: 1px solid var(--border-color);
  border-radius: 8px;
}
.disconnect-text {
  margin: 0;
  font-size: 13px;
  color: var(--text-secondary);
}
</style>
