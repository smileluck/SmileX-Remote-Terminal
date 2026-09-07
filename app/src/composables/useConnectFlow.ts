/**
 * useConnectFlow - 统一的连接流程（组件级 composable，需在 setup 中调用）
 *
 * 所有连接入口（侧边栏卡片 / ⌘K 命令面板 / 连接弹窗）共用：
 * - SSH（kind='ssh'）：session_connect 建连 + 开 tab + 启动监控
 * - 远程桌面（rdp/host）：开 tab 携带 desktopConfig，DesktopView 挂载自动连接
 * - 同 profile 已有活跃 tab → 弹「切换到已有会话 / 新建连接」
 * - 同 profile 已有断线 tab（仅 SSH）→ 复用该 tab 原地重连
 * - 否则 → 新建连接 + 开 tab + 更新最近使用
 *
 * 在 useSshConnect 之上叠加 tab 复用策略与后续动作，消除各入口行为不一致。
 */
import { useDialog, useMessage } from 'naive-ui'

import { connectProfile } from '@/composables/useSshConnect'
import { useTabsStore } from '@/stores/tabs'
import { useMonitorStore } from '@/stores/monitor'
import { useProfilesStore } from '@/stores/profiles'
import * as profileService from '@/services/profile'
import { decodeExtra } from '@/types/profile'
import type { SessionProfile } from '@/types/profile'
import type { DesktopConfig, TabItem } from '@/types/session'

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

  /** 由桌面 profile 构造 DesktopConfig（密码取 Keyring，分辨率取 extra，缺省 1920×1080/32） */
  async function buildDesktopConfig(profile: SessionProfile): Promise<DesktopConfig> {
    const password = (await profileService.getSecret(profile.id).catch(() => null)) ?? ''
    const extra = decodeExtra(profile.extra)
    return {
      kind: profile.kind === 'host' ? 'host' : 'rdp',
      host: profile.host,
      port: profile.port,
      username: profile.username,
      password,
      width: extra.width ?? 1920,
      height: extra.height ?? 1080,
      colorDepth: extra.color_depth ?? 32,
    }
  }

  /** 新建桌面连接：开 rdp/host tab（携带 desktopConfig），由 DesktopView 挂载时自动连接 */
  async function openDesktopSession(profile: SessionProfile): Promise<void> {
    const config = await buildDesktopConfig(profile)
    const tab = tabs.addTab(profile.kind, profile.name, undefined, profile.id)
    tabs.updateTab(tab.id, { desktopConfig: config })
    await profileService.touch(profile.id).catch(() => {
      /* 非关键失败：忽略 */
    })
    void profiles.loadAll()
  }

  /**
   * 统一连接入口（SSH 会话与远程桌面分流）。
   *
   * 「已有活跃 tab」走弹窗选择，错误在弹窗回调内提示；
   * 其余路径的错误向上抛出，由调用方（连接按钮的 loading 态/表单）处理。
   * 桌面 tab 没有断线重连语义（切走即断连），跳过 reconnectInTab 分支。
   */
  async function connect(profile: SessionProfile): Promise<void> {
    const isDesktop = profile.kind !== 'ssh'
    const open = () =>
      isDesktop ? openDesktopSession(profile) : openSession(profile).then(() => undefined)

    const live = tabs.tabs.find(
      (t) => t.profileId === profile.id && t.sessionId && !t.disconnected,
    )
    if (live) {
      dialog.warning({
        title: '会话已连接',
        content: `「${profile.name}」已有一个连接中的${isDesktop ? '远程桌面' : '终端'}。要切换到已有会话，还是新建一条连接？`,
        positiveText: '切换到已有会话',
        negativeText: '新建连接',
        onPositiveClick: () => switchTo(live),
        onNegativeClick: () =>
          open().catch((e) => message.error(`连接失败：${e}`)),
      })
      return
    }

    if (!isDesktop) {
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
    }

    await open()
  }

  return { connect, openSession, openDesktopSession, reconnectInTab }
}
