<script setup lang="ts">
/**
 * TopBar - 顶部工具栏（瘦身版）
 *
 * 仅承载品牌标识。新建会话入口已移至 ActivityRail，设置入口在 Rail 底部。
 * 右侧预留位（未来放全局搜索 / 主题切换）。
 */
</script>

<template>
  <header class="top-bar">
    <div class="brand">
      <span class="brand-mark">▣</span>
      <span class="brand-name">SmileX <span class="brand-sub">Remote Terminal</span></span>
    </div>
  </header>
</template>

<style scoped>
.top-bar {
  height: 44px;
  flex-shrink: 0;
  display: flex;
  align-items: center;
  padding: 0 16px;
  background: var(--bg-sidebar);
  border-bottom: 1px solid var(--border-color);
}
.brand {
  display: flex;
  align-items: center;
  gap: 8px;
}
.brand-mark {
  color: var(--primary);
  font-size: 18px;
  line-height: 1;
}
.brand-name {
  font-size: 14px;
  font-weight: 600;
  color: var(--text-primary);
  letter-spacing: 0.3px;
}
.brand-sub {
  color: var(--text-secondary);
  font-weight: 400;
}
</style>
