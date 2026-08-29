<script setup lang="ts">
/**
 * BrandMark - 品牌标识（签名元素）
 *
 * 终端提示符 ">_" 渐变圆角方块，用于 TopBar 与欢迎空状态。
 * 尺寸通过 `size` prop 控制（正方形）。
 */
withDefaults(defineProps<{ size?: number }>(), { size: 24 })
</script>

<template>
  <span class="brand-mark" :style="{ width: `${size}px`, height: `${size}px` }">
    <svg
      :width="size * 0.62"
      :height="size * 0.62"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width="2.6"
      stroke-linecap="round"
      stroke-linejoin="round"
    >
      <path d="M5 6l6 6-6 6" />
      <line x1="14" y1="18" x2="20" y2="18" />
    </svg>
  </span>
</template>

<style scoped>
.brand-mark {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
  border-radius: 28%;
  background: var(--brand-gradient);
  color: #fff;
  box-shadow:
    0 2px 8px rgba(76, 141, 255, 0.35),
    inset 0 1px 0 rgba(255, 255, 255, 0.25);
}
</style>
