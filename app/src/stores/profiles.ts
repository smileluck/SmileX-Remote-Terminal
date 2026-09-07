import { defineStore } from 'pinia'
import { ref } from 'vue'

import type { SessionProfile } from '@/types/profile'
import { decodeExtra, encodeExtra } from '@/types/profile'
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

  /** 与现有分组忽略大小写同名时沿用已有写法，避免同组因大小写分叉；空串返回 undefined */
  function normalizeGroupName(input: string): string | undefined {
    const v = input.trim()
    if (!v) return undefined
    for (const p of profiles.value) {
      const g = decodeExtra(p.extra).group?.trim() || ''
      if (g && g.toLowerCase() === v.toLowerCase()) return g
    }
    return v
  }

  /**
   * 批量把会话移动到指定分组（拖拽移动/分组合并/重命名共用）
   *
   * @param ids 目标会话 id 列表
   * @param group 新分组名；空串/undefined 表示移出分组
   * 已在同组的会话跳过写库；全部写完只刷新一次列表。
   */
  async function setGroup(ids: string[], group?: string): Promise<void> {
    const target = group?.trim() || undefined
    for (const id of ids) {
      const p = profiles.value.find((x) => x.id === id)
      if (!p) continue
      const extra = decodeExtra(p.extra)
      if ((extra.group || '').trim() === (target || '')) continue
      extra.group = target
      await profileService.save({ ...p, extra: encodeExtra(extra) })
    }
    await loadAll()
  }

  return {
    profiles,
    loaded,
    loading,
    loadAll,
    save,
    remove,
    findById,
    normalizeGroupName,
    setGroup,
  }
})
