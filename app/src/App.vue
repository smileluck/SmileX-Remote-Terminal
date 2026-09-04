<script setup lang="ts">
/**
 * App.vue - 应用根组件
 *
 * 装配 naive-ui 全局主题（深色 Termius 风）+ 消息/对话框/通知 Provider，
 * 让所有子组件能用 useMessage / useDialog / useNotification / useLoadingBar。
 *
 * 布局（水平左中右三栏）：
 * - 左：ActivityRail（功能导航窄栏）+ SideBar（会话管理，可折叠）
 * - 中：TabBar + MainContent（终端 / 远程桌面 / 设置）
 * - 右：RightPanel（Agent 助手 / 监控看版，可折叠/调宽/切页签）
 */
import { computed, onMounted, onUnmounted, ref } from 'vue'
import {
  NConfigProvider,
  NMessageProvider,
  NDialogProvider,
  NNotificationProvider,
  NLoadingBarProvider,
  darkTheme,
  type GlobalThemeOverrides,
} from 'naive-ui'
import { useThemeStore } from '@/stores/theme'
import { useTabsStore } from '@/stores/tabs'
import { useLayoutStore } from '@/stores/layout'
import { useUiStore } from '@/stores/ui'
import { useAgentStore } from '@/stores/agent'
import { useTransferStore } from '@/stores/transfer'
import TopBar from '@/components/layout/TopBar.vue'
import SideBar from '@/components/layout/SideBar.vue'
import MainContent from '@/components/layout/MainContent.vue'
import ActivityRail from '@/components/layout/ActivityRail.vue'
import TabBar from '@/components/layout/TabBar.vue'
import RightPanel from '@/components/layout/RightPanel.vue'
import SessionEvents from '@/components/layout/SessionEvents.vue'
import CommandPalette from '@/components/common/CommandPalette.vue'
import ConnectDialog from '@/components/common/ConnectDialog.vue'
import DesktopConnectDialog from '@/components/common/DesktopConnectDialog.vue'
import TransferManager from '@/components/sftp/TransferManager.vue'

const tabsStore = useTabsStore()
const layoutStore = useLayoutStore()
const uiStore = useUiStore()
// 实例化 agent store：注册 ai_token/ai_done 监听与 SSH 会话跟随（App 级一次）
useAgentStore()
// 实例化 transfer store：注册 transfer_event 监听（App 级一次）
const transferStore = useTransferStore()
transferStore.start()

const activeTab = computed(() => tabsStore.activeTab)

/** ⌘K 命令面板 */
const showPalette = ref(false)

/** 全局快捷键 */
function onKeydown(e: KeyboardEvent) {
  const mod = e.metaKey || e.ctrlKey
  if (!mod) return
  const key = e.key.toLowerCase()
  if (key === 'b') {
    e.preventDefault()
    layoutStore.toggleSidebar()
  } else if (key === 'm') {
    e.preventDefault()
    // 已在监控页签时收起，否则切到监控页签
    layoutStore.toggleRightPanel('monitor')
  } else if (key === 'j') {
    e.preventDefault()
    // 已在 Agent 页签时收起，否则切到 Agent 页签
    layoutStore.toggleRightPanel('agent')
  } else if (key === 'k') {
    e.preventDefault()
    showPalette.value = !showPalette.value
  } else if (key === 't') {
    e.preventDefault()
    uiStore.openConnectDialog()
  } else if (key === 'w') {
    e.preventDefault()
    if (tabsStore.activeId) tabsStore.closeTab(tabsStore.activeId)
  }
}

onMounted(() => window.addEventListener('keydown', onKeydown))
onUnmounted(() => window.removeEventListener('keydown', onKeydown))

/** 主题 store（dark / light / auto） */
const themeStore = useThemeStore()
/** naive-ui 主题：浅色时用 null（默认亮色），深色用 darkTheme */
const naiveTheme = computed(() => (themeStore.resolved === 'dark' ? darkTheme : null))

/** Termius 风深色主题覆盖（与 styles/main.css 的 token 对齐） */
const darkOverrides: GlobalThemeOverrides = {
  common: {
    bodyColor: '#0f1419',
    baseColor: '#0f1419',
    primaryColor: '#4c8dff',
    primaryColorHover: '#6fa5ff',
    primaryColorPressed: '#3a6fd0',
    primaryColorSuppl: '#4c8dff',
    infoColor: '#4c8dff',
    successColor: '#34d399',
    warningColor: '#fbbf24',
    errorColor: '#f87171',
    textColor1: '#e6e9ef',
    textColor2: '#aab3c0',
    textColor3: '#7d8798',
    placeholderColor: '#7d8798',
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
    borderHover: '1px solid #4c8dff',
    borderFocus: '1px solid #4c8dff',
  },
  Tag: { borderRadius: '4px' },
}

/** 浅色主题覆盖（与 .theme-light token 对齐） */
const lightOverrides: GlobalThemeOverrides = {
  common: {
    bodyColor: '#f5f7fa',
    baseColor: '#f5f7fa',
    primaryColor: '#2e6be6',
    primaryColorHover: '#4c8dff',
    primaryColorPressed: '#1f56c4',
    primaryColorSuppl: '#2e6be6',
    infoColor: '#2e6be6',
    successColor: '#059669',
    warningColor: '#d97706',
    errorColor: '#dc2626',
    textColor1: '#1f2733',
    textColor2: '#5b6575',
    textColor3: '#8b95a5',
    placeholderColor: '#8b95a5',
    borderColor: '#e3e8ef',
    dividerColor: '#e3e8ef',
    hoverColor: '#eef2f7',
    fontFamily:
      '-apple-system, BlinkMacSystemFont, "Segoe UI", "PingFang SC", "Microsoft YaHei", sans-serif',
    borderRadius: '6px',
    borderRadiusSmall: '4px',
  },
  Button: { fontWeight: '500' },
  Card: {
    color: '#ffffff',
    colorModal: '#ffffff',
    borderColor: '#e3e8ef',
  },
  Input: {
    color: '#ffffff',
    colorFocus: '#ffffff',
    borderHover: '1px solid #2e6be6',
    borderFocus: '1px solid #2e6be6',
  },
  Tag: { borderRadius: '4px' },
}

const themeOverrides = computed<GlobalThemeOverrides>(() =>
  themeStore.resolved === 'dark' ? darkOverrides : lightOverrides,
)
</script>

<template>
  <NConfigProvider :theme="naiveTheme" :theme-overrides="themeOverrides">
    <NLoadingBarProvider>
      <NMessageProvider>
        <NDialogProvider>
          <NNotificationProvider>
            <div class="app-container">
              <SessionEvents />
              <CommandPalette v-if="showPalette" @close="showPalette = false" />
              <ConnectDialog />
              <DesktopConnectDialog />
              <TopBar />
              <div class="app-body">
                <ActivityRail />
                <SideBar v-if="!layoutStore.sidebarCollapsed" />
                <div class="main-area">
                  <TabBar />
                  <MainContent :tab="activeTab" />
                </div>
                <RightPanel v-if="layoutStore.monitorVisible" />
              </div>
              <!-- 全局传输管理（右下角 FAB + 抽屉） -->
              <TransferManager />
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
