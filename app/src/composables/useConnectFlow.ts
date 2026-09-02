/**
 * useConnectFlow - 统一的 SSH 连接流程（组件级 composable，需在 setup 中调用）
 *
 * 所有连接入口（侧边栏卡片 / ⌘K 命令面板 / 连接弹窗）共用：
 * - 同 profile 已有活跃 tab → 弹「切换到已有会话 / 新建连接」
 * - 同 profile 已有断线 tab → 复用该 tab 原地重连
 * - 否则 → 新建连接 + 开 tab + 启动监控 + 更新最近使用
 *
 * 在 useSshConnect 之上叠加 tab 复用策略与后续动作，消除各入口行为不一致。
 */
import { useDialog, useMessage } from 'naive-ui'

import { connectProfile } from '@/composables/useSshConnect'
import { useTabsStore } from '@/stores/tabs'
import { useMonitorStore } from '@/stores/monitor'
import { useProfilesStore } from '@/stores/profiles'
import * as profileService from '@/services/profile'
import type { SessionProfile } from '@/types/profile'
import type { TabItem } from '@/types/session'

export function useConnectFlow() {
  const dialog = useDialog()
  const message = useMessage()
  const tabs = useTabsStore()
  const monitor = useMonitorStore()
  const profiles = useProfilesStore()

  /** 新建连接后的统一动作：开 tab + 启动监控 + 更新最近使用 */
  async function openSession(profile: SessionProfile): Promise<string> {
    const sessionId = await connectProfile(profile)
    tabs.addTab(profile.kind, profile.name, sessionId, profile.id)
    monitor.setActive(sessionId)
    await profileService.touch(profile.id).catch(() => {
      /* 非关键失败：忽略 */
    })
    void profiles.loadAll()
    return sessionId
  }

  /** 在已有 tab 上原地重连（断线恢复） */
  async function reconnectInTab(tab: TabItem, profile: SessionProfile): Promise<string> {
    const sessionId = await connectProfile(profile)
    tabs.updateTab(tab.id, { sessionId, disconnected: false, error: undefined })
    monitor.setActive(sessionId)
    await profileService.touch(profile.id).catch(() => {
      /* 非关键失败：忽略 */
    })
    return sessionId
  }

  /** 切换到已连接的 tab 并同步监控目标 */
  function switchTo(tab: TabItem) {
    tabs.setActive(tab.id)
    if (tab.sessionId) monitor.setActive(tab.sessionId)
  }

  /**
   * 统一连接入口。
   *
   * 「已有活跃 tab」走弹窗选择，错误在弹窗回调内提示；
   * 其余路径的错误向上抛出，由调用方（连接按钮的 loading 态/表单）处理。
   */
  async function connect(profile: SessionProfile): Promise<void> {
    const live = tabs.tabs.find(
      (t) => t.profileId === profile.id && t.sessionId && !t.disconnected,
    )
    if (live) {
      dialog.warning({
        title: '会话已连接',
        content: `「${profile.name}」已有一个连接中的终端。要切换到已有会话，还是新建一条连接？`,
        positiveText: '切换到已有会话',
        negativeText: '新建连接',
        onPositiveClick: () => switchTo(live),
        onNegativeClick: () =>
          openSession(profile).catch((e) => message.error(`连接失败：${e}`)),
      })
      return
    }

    const dead = tabs.tabs.find((t) => t.profileId === profile.id && t.disconnected)
    if (dead) {
      try {
        await reconnectInTab(dead, profile)
        tabs.setActive(dead.id)
        message.success(`已重新连接「${profile.name}」`)
      } catch (e) {
        message.error(`重连失败：${e}`)
      }
      return
    }

    await openSession(profile)
  }

  return { connect, openSession, reconnectInTab }
}
