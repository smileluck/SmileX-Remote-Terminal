<script setup lang="ts">
/**
 * MainContent - 主内容区
 *
 * 按 tab.kind 渲染对应视图：
 * - ssh：SSH 终端
 * - rdp / host：远程桌面
 * - settings：设置页
 * 无 active tab 时显示欢迎空状态（引导用户新建会话）。
 */
import { computed } from 'vue'
import { NButton, NIcon } from 'naive-ui'
import { Terminal2, DeviceDesktop } from '@vicons/tabler'
import type { TabItem } from '@/types/session'
import { useTabsStore } from '@/stores/tabs'
import { useUiStore } from '@/stores/ui'
import { useTabDrag } from '@/composables/useTabDrag'
import BrandMark from '@/components/common/BrandMark.vue'
import TerminalView from '@/components/terminal/TerminalView.vue'
import DesktopView from '@/components/desktop/DesktopView.vue'
import SettingsView from '@/components/settings/SettingsView.vue'

defineProps<{ tab: TabItem | null }>()
const tabs = useTabsStore()
const ui = useUiStore()
const tabDrag = useTabDrag()

/** 所有 SSH tab：常驻渲染、按需显示，保证每个 tab 的终端实例与分屏状态完全独立 */
const sshTabs = computed(() => tabs.tabs.filter((t) => t.kind === 'ssh'))

/**
 * 是否显示 tab 拖放区：拖拽中 + 目标是「其他」SSH tab（不能拖到自身视图）
 * + 目标视图窗格数未达上限（窗格数由 TerminalView 上报到 useTabDrag）
 */
const dropVisible = computed(() => {
  const d = tabDrag.draggingTab.value
  const active = tabs.activeTab
  if (!d || !active || active.kind !== 'ssh' || d.id === active.id) return false
  return (tabDrag.paneCounts.get(active.id) ?? 1) < 4
})
</script>

<template>
  <div class="main-content">
    <!-- SSH 终端：每个 tab 常驻一个独立视图，切换仅隐藏/显示（保留终端内容与会话绑定） -->
    <div
      v-for="t in sshTabs"
      :key="t.id"
      v-show="t.id === tab?.id"
      class="tab-view"
    >
      <TerminalView :tab="t" />
    </div>

    <template v-if="tab">
      <DesktopView v-if="tab.kind === 'rdp' || tab.kind === 'host'" :tab="tab" />
      <SettingsView v-else-if="tab.kind === 'settings'" />
    </template>

    <div v-else class="empty-wrap">
      <div class="empty-body">
        <BrandMark :size="64" class="empty-logo" />
        <h2>开始你的第一个连接</h2>
        <p class="empty-hint">SSH 终端 · 远程桌面 · AI 运维助手</p>
        <div class="empty-actions">
          <NButton type="primary" size="large" @click="ui.openConnectDialog()">
            <template #icon><NIcon :component="Terminal2" /></template>
            新建 SSH
          </NButton>
          <NButton size="large" @click="ui.openDesktopConnectDialog()">
            <template #icon><NIcon :component="DeviceDesktop" /></template>
            远程桌面
          </NButton>
        </div>
      </div>
    </div>

    <!-- Tab 拖放区：拖到某一边缘即把该 tab 的会话并入当前视图为分屏 pane（命中检测在 TabBar） -->
    <div v-if="dropVisible" class="tab-drop-overlay" data-tab-drop>
      <div class="tab-drop-zone zone-left" :class="{ on: tabDrag.dropZone.value === 'left' }" />
      <div class="tab-drop-zone zone-right" :class="{ on: tabDrag.dropZone.value === 'right' }" />
      <div class="tab-drop-zone zone-top" :class="{ on: tabDrag.dropZone.value === 'top' }" />
      <div class="tab-drop-zone zone-bottom" :class="{ on: tabDrag.dropZone.value === 'bottom' }" />
    </div>
  </div>
</template>

<style scoped>
.main-content {
  flex: 1;
  overflow: hidden;
  display: flex;
  position: relative;
  background: var(--bg-app);
}
.tab-drop-overlay {
  position: absolute;
  inset: 0;
  z-index: 20;
}
.tab-drop-zone {
  position: absolute;
  pointer-events: none;
  background: rgba(76, 141, 255, 0.1);
  border: 1px dashed rgba(76, 141, 255, 0.4);
  border-radius: 6px;
  transition:
    background 0.12s,
    border-color 0.12s;
}
.tab-drop-zone.on {
  background: rgba(76, 141, 255, 0.28);
  border: 1px solid var(--primary);
}
.zone-left {
  left: 4px;
  top: 4px;
  bottom: 4px;
  width: 30%;
}
.zone-right {
  right: 4px;
  top: 4px;
  bottom: 4px;
  width: 30%;
}
.zone-top {
  left: calc(30% + 8px);
  right: calc(30% + 8px);
  top: 4px;
  height: 30%;
}
.zone-bottom {
  left: calc(30% + 8px);
  right: calc(30% + 8px);
  bottom: 4px;
  height: 30%;
}
.tab-view {
  flex: 1;
  min-width: 0;
  min-height: 0;
  display: flex;
}
.empty-wrap {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  /* 微弱的径向光晕，给空状态一点空间感 */
  background: radial-gradient(
      ellipse 60% 50% at 50% 40%,
      rgba(76, 141, 255, 0.06),
      transparent 70%
    );
}
.empty-body {
  display: flex;
  flex-direction: column;
  align-items: center;
  text-align: center;
}
.empty-logo {
  margin-bottom: 24px;
  filter: drop-shadow(0 8px 24px rgba(76, 141, 255, 0.35));
}
.empty-body h2 {
  font-size: 20px;
  font-weight: 600;
  color: var(--text-primary);
  letter-spacing: 0.2px;
  margin-bottom: 8px;
}
.empty-hint {
  font-size: 13px;
  color: var(--text-secondary);
  margin-bottom: 32px;
}
.empty-actions {
  display: flex;
  gap: 12px;
}
</style>
