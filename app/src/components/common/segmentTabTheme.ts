/**
 * 右栏面板通用 segment 页签主题（紧凑尺寸）
 *
 * 走 naive-ui 组件级 theme-overrides（CSS 变量），
 * 避免与 naive-ui 运行时注入的高优先级 CSS 拼选择器。
 */
export const segmentTabThemeOverrides = {
  tabPaddingSmallSegment: '2px 0',
  tabFontSizeSmall: '12px',
}
