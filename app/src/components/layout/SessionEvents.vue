<script setup lang="ts">
/**
 * SessionEvents - 全局会话事件监听
 *
 * 必须挂在 NotificationProvider/DialogProvider 内部（use* hook 约束）：
 * - `session_closed`：会话意外断开 → tab 标记 disconnected + 桌面通知
 * - `hostkey_confirm`：未知主机首次连接 → 指纹确认对话框（60s 超时自动拒绝）
 * - 初始化监控 store 的事件订阅
 */
import { onMounted } from 'vue'
import { useNotification, useDialog } from 'naive-ui'
import { listen, invoke } from '@/services/invoke'
import { useTabsStore } from '@/stores/tabs'
import { useMonitorStore } from '@/stores/monitor'

const tabs = useTabsStore()
const monitor = useMonitorStore()
const notification = useNotification()
const dialog = useDialog()

interface HostKeyConfirmPayload {
  request_id: string
  host: string
  port: number
  key_type: string
  fingerprint: string
}

/** 未知主机指纹确认对话框 */
function showHostKeyConfirm(p: HostKeyConfirmPayload) {
  const respond = (accept: boolean) => {
    invoke('host_key_respond', { requestId: p.request_id, accept }).catch(() => {})
  }
  dialog.warning({
    title: '验证主机指纹',
    content: () =>
      `首次连接该主机，请核对指纹（SHA-256）：\n\n` +
      `${p.host}:${p.port}\n` +
      `算法：${p.key_type}\n` +
      `指纹：${p.fingerprint}\n\n` +
      `信任并继续连接吗？（60 秒未确认将自动拒绝）`,
    positiveText: '信任并连接',
    negativeText: '拒绝',
    onPositiveClick: () => respond(true),
    onNegativeClick: () => respond(false),
    onClose: () => respond(false),
    onMaskClick: () => respond(false),
  })
}

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

  await listen<HostKeyConfirmPayload>('hostkey_confirm', showHostKeyConfirm)
})
</script>

<template></template>
