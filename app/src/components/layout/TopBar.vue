<script setup lang="ts">
/**
 * TopBar - 顶部工具栏（品牌标识）
 *
 * 布局开关已分散至各栏：折叠按钮在左侧活动栏，监控/AI 在终端工具组。
 */
import BrandMark from '@/components/common/BrandMark.vue'
</script>

<template>
  <header class="top-bar">
    <div class="brand">
      <BrandMark :size="22" />
      <span class="brand-name">SmileX <span class="brand-sub">Remote Terminal</span></span>
    </div>
  </header>
</template>

<style scoped>
.top-bar {
  height: 48px;
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 12px 0 16px;
  background: var(--bg-sidebar);
  border-bottom: 1px solid var(--border-color);
}
.brand {
  display: flex;
  align-items: center;
  gap: 10px;
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
