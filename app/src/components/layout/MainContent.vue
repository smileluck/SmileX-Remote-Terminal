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
import { NButton, NIcon } from 'naive-ui'
import { Terminal2, DeviceDesktop, Robot } from '@vicons/tabler'
import type { TabItem } from '@/types/session'
import { useTabsStore } from '@/stores/tabs'
import { useUiStore } from '@/stores/ui'
import { useLayoutStore } from '@/stores/layout'
import BrandMark from '@/components/common/BrandMark.vue'
import TerminalView from '@/components/terminal/TerminalView.vue'
import DesktopView from '@/components/desktop/DesktopView.vue'
import SettingsView from '@/components/settings/SettingsView.vue'

defineProps<{ tab: TabItem | null }>()
const tabs = useTabsStore()
const ui = useUiStore()
const layout = useLayoutStore()
</script>

<template>
  <div class="main-content">
    <template v-if="tab">
      <TerminalView v-if="tab.kind === 'ssh'" :tab="tab" />
      <DesktopView v-else-if="tab.kind === 'rdp' || tab.kind === 'host'" :tab="tab" />
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
          <NButton size="large" @click="tabs.addTab('rdp', '新远程桌面')">
            <template #icon><NIcon :component="DeviceDesktop" /></template>
            远程桌面
          </NButton>
          <NButton size="large" @click="layout.openRightPanel('agent')">
            <template #icon><NIcon :component="Robot" /></template>
            AI 助手
          </NButton>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.main-content {
  flex: 1;
  overflow: hidden;
  display: flex;
  background: var(--bg-app);
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
