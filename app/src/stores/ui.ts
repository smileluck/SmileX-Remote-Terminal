import { defineStore } from 'pinia'
import { ref } from 'vue'

/**
 * 全局 UI 状态 store
 *
 * - 连接弹窗：全应用唯一的新建/编辑 SSH 会话入口
 *   （欢迎页 / ⌘T / 终端窗格等入口都打开这同一个弹窗）
 * - 远程桌面连接弹窗：新建 rdp/host 连接的唯一入口
 *   （会话列表「+」下拉 / 欢迎页打开）
 */
export const useUiStore = defineStore('ui', () => {
  /** 连接弹窗是否可见 */
  const connectVisible = ref(false)
  /** 编辑模式：目标 profile id（null = 新建） */
  const connectProfileId = ref<string | null>(null)

  /** 打开连接弹窗（传入 profileId 进入编辑模式） */
  function openConnectDialog(profileId?: string) {
    connectProfileId.value = profileId ?? null
    connectVisible.value = true
  }

  /** 关闭连接弹窗 */
  function closeConnectDialog() {
    connectVisible.value = false
    connectProfileId.value = null
  }

  /** 远程桌面连接弹窗是否可见 */
  const desktopConnectVisible = ref(false)

  /** 打开远程桌面连接弹窗 */
  function openDesktopConnectDialog() {
    desktopConnectVisible.value = true
  }

  /** 关闭远程桌面连接弹窗 */
  function closeDesktopConnectDialog() {
    desktopConnectVisible.value = false
  }

  return {
    connectVisible,
    connectProfileId,
    openConnectDialog,
    closeConnectDialog,
    desktopConnectVisible,
    openDesktopConnectDialog,
    closeDesktopConnectDialog,
  }
})
