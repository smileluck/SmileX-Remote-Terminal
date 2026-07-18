<script setup lang="ts">
/**
 * App.vue - 应用根组件
 *
 * 布局：顶部工具栏 + 侧边栏 + 主内容区
 * 主内容区按当前活动 Tab 的 kind 渲染对应视图（终端/远程桌面/AI/管理）
 */
import { computed } from 'vue'
import { useTabsStore } from '@/stores/tabs'
import TopBar from '@/components/layout/TopBar.vue'
import SideBar from '@/components/layout/SideBar.vue'
import MainContent from '@/components/layout/MainContent.vue'

const tabsStore = useTabsStore()
const activeTab = computed(() => tabsStore.activeTab)
</script>

<template>
  <div class="app-container">
    <TopBar />
    <div class="app-body">
      <SideBar />
      <MainContent :tab="activeTab" />
    </div>
  </div>
</template>

<style scoped>
.app-container {
  display: flex;
  flex-direction: column;
  height: 100vh;
  width: 100vw;
  overflow: hidden;
}
.app-body {
  display: flex;
  flex: 1;
  overflow: hidden;
}
</style>
