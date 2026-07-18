<script setup lang="ts">
/**
 * ContextToggle - "附带当前会话上下文"开关
 *
 * 勾选后 AI 会结合当前 SSH 终端输出作答
 */
const model = defineModel<boolean>({ default: false })
</script>

<template>
  <label class="ctx-toggle">
    <input type="checkbox" v-model="model" />
    <span>附带上下文</span>
  </label>
</template>

<style scoped>
.ctx-toggle {
  display: flex;
  align-items: center;
  gap: 4px;
  font-size: 12px;
  color: #666;
  cursor: pointer;
}
.ctx-toggle input {
  margin: 0;
}
</style>
