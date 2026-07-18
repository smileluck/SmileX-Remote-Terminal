<script setup lang="ts">
/**
 * ConnectForm - 空状态占位组件
 *
 * 显示欢迎信息与快速新建入口
 */
</script>

<template>
  <div class="connect-form">
    <h1>欢迎使用 SmileX Remote Terminal</h1>
    <p>跨平台综合运维工具：SSH 终端 + 远程桌面 + AI 助手</p>
    <div class="hint">点击顶部工具栏的"+ SSH / + 桌面 / + AI"新建会话</div>
  </div>
</template>

<style scoped>
.connect-form {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  color: #666;
}
.connect-form h1 {
  color: var(--primary-color);
  margin-bottom: 8px;
}
.hint {
  margin-top: 16px;
  color: #999;
  font-size: 13px;
}
</style>
