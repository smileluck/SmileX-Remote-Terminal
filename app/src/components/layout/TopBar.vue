<script setup lang="ts">
/**
 * TopBar - 顶部工具栏
 *
 * 含应用名 + 快速新建按钮 + 设置入口
 */
import { useTabsStore } from '@/stores/tabs'

const tabs = useTabsStore()

function newSsh() {
  tabs.addTab('ssh', '新 SSH 会话')
}
function newDesktop() {
  tabs.addTab('rdp', '新远程桌面')
}
function newChat() {
  tabs.addTab('chat', 'AI 助手')
}
</script>

<template>
  <div class="top-bar">
    <div class="brand">SmileX Remote Terminal</div>
    <div class="actions">
      <button @click="newSsh">+ SSH</button>
      <button @click="newDesktop">+ 桌面</button>
      <button @click="newChat">+ AI</button>
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
.actions button {
  margin-left: 8px;
  padding: 4px 12px;
  border: 1px solid var(--border-color);
  border-radius: 4px;
  background: #fff;
  cursor: pointer;
}
.actions button:hover {
  background: var(--tab-hover-bg);
}
</style>
