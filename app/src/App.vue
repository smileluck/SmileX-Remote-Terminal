<script setup lang="ts">
/**
 * App.vue - 应用根组件
 *
 * 装配 naive-ui 全局主题（深色 Termius 风）+ 消息/对话框/通知 Provider，
 * 让所有子组件能用 useMessage / useDialog / useNotification / useLoadingBar。
 *
 * 注意：这些 hook 必须在对应 Provider 的【子组件】setup 中调用，App.vue 自身不能用。
 *
 * 布局（PR2 将重构为 Termius 四区：rail + sessions + tabbar + 主区）：
 * 顶部工具栏 + 侧边栏 + 主内容区。
 */
import { computed } from 'vue'
import {
  NConfigProvider,
  NMessageProvider,
  NDialogProvider,
  NNotificationProvider,
  NLoadingBarProvider,
  darkTheme,
  type GlobalThemeOverrides,
} from 'naive-ui'
import { useTabsStore } from '@/stores/tabs'
import TopBar from '@/components/layout/TopBar.vue'
import SideBar from '@/components/layout/SideBar.vue'
import MainContent from '@/components/layout/MainContent.vue'
import ActivityRail from '@/components/layout/ActivityRail.vue'
import TabBar from '@/components/layout/TabBar.vue'

const tabsStore = useTabsStore()
const activeTab = computed(() => tabsStore.activeTab)

/** Termius 风深色主题覆盖（与 styles/main.css 的 token 对齐） */
const themeOverrides: GlobalThemeOverrides = {
  common: {
    bodyColor: '#0f1419',
    baseColor: '#0f1419',
    primaryColor: '#3a7afe',
    primaryColorHover: '#5a8fff',
    primaryColorPressed: '#2a62d8',
    primaryColorSuppl: '#3a7afe',
    infoColor: '#3a7afe',
    successColor: '#34d399',
    warningColor: '#fbbf24',
    errorColor: '#f87171',
    textColor1: '#e6e9ef',
    textColor2: '#a8b0bd',
    textColor3: '#6b7280',
    placeholderColor: '#6b7280',
    borderColor: '#1f2731',
    dividerColor: '#1f2731',
    hoverColor: '#1a2230',
    fontFamily:
      '-apple-system, BlinkMacSystemFont, "Segoe UI", "PingFang SC", "Microsoft YaHei", sans-serif',
    borderRadius: '6px',
    borderRadiusSmall: '4px',
  },
  Button: { fontWeight: '500' },
  Card: {
    color: '#161c24',
    colorModal: '#1a2230',
    borderColor: '#1f2731',
  },
  Input: {
    color: '#161c24',
    colorFocus: '#1a2230',
    borderHover: '1px solid #3a7afe',
    borderFocus: '1px solid #3a7afe',
  },
  Tag: { borderRadius: '4px' },
}
</script>

<template>
  <NConfigProvider :theme="darkTheme" :theme-overrides="themeOverrides">
    <NLoadingBarProvider>
      <NMessageProvider>
        <NDialogProvider>
          <NNotificationProvider>
            <div class="app-container">
              <TopBar />
              <div class="app-body">
                <ActivityRail />
                <SideBar />
                <div class="main-area">
                  <TabBar />
                  <MainContent :tab="activeTab" />
                </div>
              </div>
            </div>
          </NNotificationProvider>
        </NDialogProvider>
      </NMessageProvider>
    </NLoadingBarProvider>
  </NConfigProvider>
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
.main-area {
  display: flex;
  flex-direction: column;
  flex: 1;
  min-width: 0;
  overflow: hidden;
}
</style>
