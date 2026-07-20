<script setup lang="ts">
/**
 * ContextToggle - "附带当前会话上下文"开关
 *
 * 开启后 AI 会结合当前 SSH 终端输出作答。
 */
import { NSwitch } from 'naive-ui'

const model = defineModel<boolean>({ default: false })
</script>

<template>
  <div class="ctx-toggle">
    <NSwitch v-model:value="model" size="small" />
    <span class="ctx-label">附带上下文</span>
  </div>
</template>

<style scoped>
.ctx-toggle {
  display: flex;
  align-items: center;
  gap: 6px;
}
.ctx-label {
  font-size: 12px;
  color: var(--text-secondary);
  white-space: nowrap;
}
</style>
