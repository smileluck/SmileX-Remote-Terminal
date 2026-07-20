<script setup lang="ts">
/**
 * MainContent - 主内容区
 *
 * 按 tab.kind 渲染对应视图：
 * - ssh：SSH 终端
 * - rdp / host：远程桌面
 * - chat：AI 助手
 * - settings：设置页
 * 无 active tab 时显示欢迎空状态（引导用户新建会话）。
 */
import { NButton, NIcon, NEmpty, NSpace } from 'naive-ui'
import { Terminal2, DeviceDesktop, Robot } from '@vicons/tabler'
import type { TabItem } from '@/types/session'
import { useTabsStore } from '@/stores/tabs'
import TerminalView from '@/components/terminal/TerminalView.vue'
import DesktopView from '@/components/desktop/DesktopView.vue'
import ChatPanel from '@/components/ai/ChatPanel.vue'
import SettingsView from '@/components/settings/SettingsView.vue'

defineProps<{ tab: TabItem | null }>()
const tabs = useTabsStore()
</script>

<template>
  <div class="main-content">
    <template v-if="tab">
      <TerminalView v-if="tab.kind === 'ssh'" :tab="tab" />
      <DesktopView v-else-if="tab.kind === 'rdp' || tab.kind === 'host'" :tab="tab" />
      <ChatPanel v-else-if="tab.kind === 'chat'" :tab="tab" />
      <SettingsView v-else-if="tab.kind === 'settings'" />
    </template>

    <div v-else class="empty-wrap">
      <NEmpty size="large">
        <template #icon>
          <span class="empty-logo">▣</span>
        </template>
        <div class="empty-body">
          <h2>欢迎使用 SmileX Remote Terminal</h2>
          <p class="empty-hint">SSH 终端 · 远程桌面 · AI 运维助手</p>
          <NSpace>
            <NButton type="primary" @click="tabs.addTab('ssh', '新 SSH 会话')">
              <template #icon><NIcon :component="Terminal2" /></template>
              新建 SSH
            </NButton>
            <NButton @click="tabs.addTab('rdp', '新远程桌面')">
              <template #icon><NIcon :component="DeviceDesktop" /></template>
              远程桌面
            </NButton>
            <NButton @click="tabs.addTab('chat', 'AI 助手')">
              <template #icon><NIcon :component="Robot" /></template>
              AI 助手
            </NButton>
          </NSpace>
        </div>
      </NEmpty>
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
}
.empty-logo {
  font-size: 56px;
  color: var(--primary);
  opacity: 0.45;
  line-height: 1;
}
.empty-body {
  text-align: center;
  margin-top: 12px;
}
.empty-body h2 {
  font-size: 18px;
  font-weight: 600;
  color: var(--text-primary);
  margin-bottom: 6px;
}
.empty-hint {
  font-size: 13px;
  color: var(--text-secondary);
  margin-bottom: 20px;
}
</style>
