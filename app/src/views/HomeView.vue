<script setup lang="ts">
/**
 * HomeView.vue - 占位首页
 *
 * MVP 阶段：显示欢迎信息。后续会话/桌面/AI 都通过 Tab 承载，不走路由。
 */
</script>

<template>
  <div class="home-view">
    <h1>SmileX Remote Terminal</h1>
    <p>SSH 终端 + 远程桌面 + AI 运维助手</p>
    <p class="hint">通过侧边栏新建会话开始使用</p>
  </div>
</template>

<style scoped>
.home-view {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  height: 100%;
  color: #888;
}
.home-view h1 {
  font-size: 28px;
  margin-bottom: 12px;
  color: #2080f0;
}
.home-view .hint {
  margin-top: 24px;
  font-size: 13px;
}
</style>
