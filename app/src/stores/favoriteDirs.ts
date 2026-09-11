import { defineStore } from 'pinia'
import { ref } from 'vue'

import type { FavoriteDir } from '@/services/snippets'
import {
  favoriteDirList,
  favoriteDirSave,
  favoriteDirDelete,
  favoriteDirReorder,
} from '@/services/snippets'
import { useTabsStore } from '@/stores/tabs'

/**
 * 常用目录 store（常用记录面板「目录」Tab）
 *
 * 按主机档案（profileId）隔离：激活 Tab 切换主机时重新加载。
 * 无 profile 的临时会话归入空串桶。
 */
export const useFavoriteDirsStore = defineStore('favoriteDirs', () => {
  /** 当前主机档案的常用目录（按 sortOrder 升序） */
  const dirs = ref<FavoriteDir[]>([])
  /** 已加载的 profileId（null = 未加载） */
  const loadedFor = ref<string | null>(null)

  /** 当前激活会话的主机档案 ID（无 profile = ''） */
  function activeProfileId(): string {
    const tabs = useTabsStore()
    return tabs.activeTab?.profileId ?? ''
  }

  /** 加载当前主机档案的常用目录（profileId 变化时强制重载） */
  async function load(force = false) {
    const pid = activeProfileId()
    if (!force && loadedFor.value === pid) return
    try {
      const list = await favoriteDirList(pid)
      dirs.value = [...list].sort((a, b) => a.sortOrder - b.sortOrder || a.createdAt - b.createdAt)
      loadedFor.value = pid
    } catch {
      /* 浏览器 dev 环境无后端时静默 */
    }
  }

  /** 保存/更新（成功后重载当前列表） */
  async function save(dir: FavoriteDir): Promise<void> {
    await favoriteDirSave(dir)
    await load(true)
  }

  /** 删除（成功后重载当前列表） */
  async function remove(id: string): Promise<void> {
    await favoriteDirDelete(id)
    await load(true)
  }

  /** 按内存顺序重编 sortOrder 并整体落库 */
  async function persistOrder(): Promise<void> {
    let i = 0
    const renumbered = dirs.value.map((d) => ({ ...d, sortOrder: i++ }))
    dirs.value = renumbered
    await favoriteDirReorder(renumbered)
  }

  return { dirs, loadedFor, activeProfileId, load, save, remove, persistOrder }
})
