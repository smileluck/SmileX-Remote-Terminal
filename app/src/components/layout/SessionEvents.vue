<script setup lang="ts">
/**
 * SessionEvents - 全局会话事件监听
 *
 * 必须挂在 NotificationProvider 内部（useNotification 约束）：
 * - `session_closed`：会话意外断开 → tab 标记 disconnected + 桌面通知
 * - 初始化监控 store 的事件订阅
 */
import { onMounted } from 'vue'
import { useNotification } from 'naive-ui'
import { listen } from '@/services/invoke'
import { useTabsStore } from '@/stores/tabs'
import { useMonitorStore } from '@/stores/monitor'

const tabs = useTabsStore()
const monitor = useMonitorStore()
const notification = useNotification()

onMounted(async () => {
  monitor.init()

  await listen<{ session_id: string }>('session_closed', ({ session_id }) => {
    // tab 已被用户主动关闭时静默忽略
    const tab = tabs.tabs.find((t) => t.sessionId === session_id)
    if (!tab || tab.disconnected) return

    tabs.updateTab(tab.id, { disconnected: true })
    monitor.stopSampling(session_id)
    notification.warning({
      title: '会话已断开',
      content: `「${tab.title}」的连接已关闭，可在终端视图点击「重新连接」`,
      duration: 6000,
    })
  })
})
</script>

<template></template>
