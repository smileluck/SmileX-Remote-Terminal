<script setup lang="ts">
/**
 * SideBar - 侧边栏
 *
 * 显示所有打开的标签，可切换/关闭
 */
import { useTabsStore } from '@/stores/tabs'
import type { TabItem } from '@/types/session'

const tabs = useTabsStore()

function kindLabel(kind: TabItem['kind']): string {
  return { ssh: 'SSH', rdp: '桌面', host: 'Mac', chat: 'AI' }[kind]
}
</script>

<template>
  <div class="side-bar">
    <div
      v-for="tab in tabs.tabs"
      :key="tab.id"
      class="tab-item"
      :class="{ active: tab.id === tabs.activeId }"
      @click="tabs.setActive(tab.id)"
    >
      <span class="kind-tag" :data-kind="tab.kind">{{ kindLabel(tab.kind) }}</span>
      <span class="title">{{ tab.title }}</span>
      <button class="close-btn" @click.stop="tabs.closeTab(tab.id)">×</button>
    </div>
    <div v-if="tabs.tabs.length === 0" class="empty">暂无打开的会话</div>
  </div>
</template>

<style scoped>
.side-bar {
  width: 200px;
  background: var(--tab-active-bg);
  border-right: 1px solid var(--border-color);
  padding: 8px 0;
  overflow-y: auto;
}
.tab-item {
  display: flex;
  align-items: center;
  padding: 6px 12px;
  cursor: pointer;
  font-size: 13px;
}
.tab-item:hover {
  background: var(--tab-hover-bg);
}
.tab-item.active {
  background: #e8f4ff;
  border-left: 3px solid var(--primary-color);
}
.kind-tag {
  padding: 1px 6px;
  border-radius: 3px;
  font-size: 11px;
  margin-right: 6px;
  background: #eee;
}
.kind-tag[data-kind='ssh'] { background: #d4edda; color: #155724; }
.kind-tag[data-kind='rdp'] { background: #cce5ff; color: #004085; }
.kind-tag[data-kind='host'] { background: #f8d7da; color: #721c24; }
.kind-tag[data-kind='chat'] { background: #fff3cd; color: #856404; }
.title {
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.close-btn {
  background: none;
  border: none;
  cursor: pointer;
  font-size: 16px;
  color: #999;
}
.empty {
  padding: 16px;
  color: #999;
  font-size: 12px;
  text-align: center;
}
</style>
