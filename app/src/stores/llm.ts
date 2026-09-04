import { defineStore } from 'pinia'
import { computed, ref } from 'vue'

import * as settingsService from '@/services/settings'
import type { LlmProfile } from '@/types/settings'

/**
 * LLM 配置档案 store（全局单例）
 *
 * 供 Agent 面板的模型切换等快捷入口共用：
 * - profiles/activeId 常驻内存，ChatPanel 打开即加载，切换即时生效
 * - switchActive 调后端排他激活（同步应用到 ChatProvider，下一轮对话生效）
 *
 * 注：设置页编辑档案仍走自身的本地状态（按 tab 挂载、每次打开重新拉取），
 * 两处数据源均为后端命令，不会长期漂移。
 */
export const useLlmStore = defineStore('llm', () => {
  /** 所有配置档案（不含 API Key） */
  const profiles = ref<LlmProfile[]>([])
  /** 当前激活档案 id（null = 无激活/未加载） */
  const activeId = ref<string | null>(null)
  /** 首次加载是否完成 */
  const loaded = ref(false)
  /** 切换中的档案 id（用于下拉 loading 态） */
  const switchingId = ref<string | null>(null)

  const activeProfile = computed(
    () => profiles.value.find((p) => p.id === activeId.value) ?? null,
  )

  /** 拉取档案列表与激活态（可重复调用刷新） */
  async function load() {
    try {
      const [list, active] = await Promise.all([
        settingsService.listProfiles(),
        settingsService.getActiveProfile(),
      ])
      profiles.value = list
      activeId.value = active?.id ?? null
    } finally {
      loaded.value = true
    }
  }

  /** 切换激活档案（排他；失败时保持原激活态） */
  async function switchActive(id: string) {
    if (id === activeId.value) return
    switchingId.value = id
    try {
      await settingsService.setActiveProfile(id)
      activeId.value = id
    } finally {
      switchingId.value = null
    }
  }

  return {
    profiles,
    activeId,
    loaded,
    switchingId,
    activeProfile,
    load,
    switchActive,
  }
})
