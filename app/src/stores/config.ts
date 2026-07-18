import { defineStore } from 'pinia'
import { ref } from 'vue'

import type { LlmProviderConfig } from '@/types/ai'
import * as aiService from '@/services/ai'

/**
 * 应用配置 store
 *
 * 管理全局配置（主题、LLM Provider 等）。
 * LLM API Key 存 Tauri Keyring（通过 services.ai 更新）。
 */
export const useConfigStore = defineStore('config', () => {
  /** 当前 LLM 配置 */
  const llmConfig = ref<LlmProviderConfig | null>(null)
  /** 是否正在初始化 */
  const initialized = ref(false)

  /** 更新 LLM 配置 */
  async function updateLlmConfig(config: LlmProviderConfig) {
    // 同步到后端（后端重建 LlmClient）
    await aiService.updateConfig(config)
    llmConfig.value = config
  }

  return {
    llmConfig,
    initialized,
    updateLlmConfig,
  }
})
