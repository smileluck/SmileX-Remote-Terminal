<script setup lang="ts">
/**
 * MainContent - 主内容区
 *
 * 按 tab.kind 渲染对应视图：
 * - ssh：SSH 终端
 * - rdp / host：远程桌面
 * - chat：AI 助手
 * - settings：设置页（阶段 5）
 */
import type { TabItem } from '@/types/session'
import TerminalView from '@/components/terminal/TerminalView.vue'
import DesktopView from '@/components/desktop/DesktopView.vue'
import ChatPanel from '@/components/ai/ChatPanel.vue'
import SettingsView from '@/components/settings/SettingsView.vue'
import ConnectForm from '@/components/common/ConnectForm.vue'

const props = defineProps<{ tab: TabItem | null }>()
</script>

<template>
  <div class="main-content">
    <template v-if="tab">
      <!-- SSH 终端 -->
      <TerminalView v-if="tab.kind === 'ssh'" :tab="tab" />

      <!-- 远程桌面（rdp / host） -->
      <DesktopView v-else-if="tab.kind === 'rdp' || tab.kind === 'host'" :tab="tab" />

      <!-- AI Chat -->
      <ChatPanel v-else-if="tab.kind === 'chat'" :tab="tab" />

      <!-- 设置 -->
      <SettingsView v-else-if="tab.kind === 'settings'" />
    </template>

    <!-- 默认：连接管理 -->
    <ConnectForm v-else />
  </div>
</template>

<style scoped>
.main-content {
  flex: 1;
  overflow: hidden;
  display: flex;
}
</style>
