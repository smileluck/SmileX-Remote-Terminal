import { defineStore } from 'pinia'
import { ref } from 'vue'

import type { SessionProfile } from '@/types/profile'
import * as profileService from '@/services/profile'

/**
 * 会话配置 store
 *
 * 加载并缓存所有已保存的 SessionProfile 供侧栏展示。
 * 保存/删除时自动同步列表。
 */
export const useProfilesStore = defineStore('profiles', () => {
  /** 所有会话配置（按最近使用排序） */
  const profiles = ref<SessionProfile[]>([])
  /** 是否已初始化 */
  const loaded = ref(false)
  /** 是否正在加载 */
  const loading = ref(false)

  /** 加载所有会话配置 */
  async function loadAll() {
    if (loading.value) return
    loading.value = true
    try {
      profiles.value = await profileService.list()
      loaded.value = true
    } finally {
      loading.value = false
    }
  }

  /** 保存或更新（成功后刷新列表） */
  async function save(
    profile: SessionProfile,
    secret?: string | null,
  ): Promise<SessionProfile> {
    const saved = await profileService.save(profile, secret)
    await loadAll()
    return saved
  }

  /** 删除（成功后刷新列表） */
  async function remove(id: string): Promise<boolean> {
    const ok = await profileService.remove(id)
    if (ok) await loadAll()
    return ok
  }

  /** 按 id 查询（本地缓存，未命中返回 null） */
  function findById(id: string): SessionProfile | undefined {
    return profiles.value.find((p) => p.id === id)
  }

  return {
    profiles,
    loaded,
    loading,
    loadAll,
    save,
    remove,
    findById,
  }
})
