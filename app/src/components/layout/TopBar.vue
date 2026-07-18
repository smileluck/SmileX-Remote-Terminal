<script setup lang="ts">
/**
 * TopBar - 顶部工具栏
 *
 * 含应用名 + 快速新建按钮 + 设置入口
 *
 * 设置 Tab 单例：重复点击不会新建多个，已存在则激活。
 */
import { useTabsStore } from '@/stores/tabs'

const tabs = useTabsStore()

/** 新建 SSH 会话 Tab */
function newSsh() {
  tabs.addTab('ssh', '新 SSH 会话')
}

/** 新建远程桌面 Tab */
function newDesktop() {
  tabs.addTab('rdp', '新远程桌面')
}

/** 新建 AI Chat Tab */
function newChat() {
  tabs.addTab('chat', 'AI 助手')
}

/**
 * 打开设置 Tab（单例）
 *
 * 查找已有 settings kind 的 Tab：
 * - 存在：激活
 * - 不存在：新建
 */
function openSettings() {
  const existing = tabs.tabs.find((t) => t.kind === 'settings')
  if (existing) {
    tabs.setActive(existing.id)
  } else {
    tabs.addTab('settings', '设置')
  }
}
</script>

<template>
  <div class="top-bar">
    <div class="brand">SmileX Remote Terminal</div>
    <div class="actions">
      <button @click="newSsh">+ SSH</button>
      <button @click="newDesktop">+ 桌面</button>
      <button @click="newChat">+ AI</button>
      <button
        class="settings-btn"
        title="设置"
        @click="openSettings"
      >⚙</button>
    </div>
  </div>
</template>

<style scoped>
.top-bar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 16px;
  height: 40px;
  background: var(--tab-active-bg);
  border-bottom: 1px solid var(--border-color);
}
.brand {
  font-weight: 600;
  color: var(--primary-color);
}
.actions {
  display: flex;
  align-items: center;
  gap: 8px;
}
.actions button {
  padding: 4px 12px;
  border: 1px solid var(--border-color);
  border-radius: 4px;
  background: #fff;
  cursor: pointer;
  font-size: 13px;
}
.actions button:hover {
  background: var(--tab-hover-bg);
}
.settings-btn {
  width: 32px;
  height: 28px;
  padding: 0 !important;
  font-size: 16px;
  line-height: 1;
}
</style>
