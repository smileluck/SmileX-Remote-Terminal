import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import { resolve } from 'path'

// Vite 配置：Vue3 + Tauri 集成
export default defineConfig({
  plugins: [vue()],
  resolve: {
    alias: {
      '@': resolve(__dirname, 'src'),
    },
  },
  // Tauri 期望前端在固定端口启动
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
  },
  // 生产构建产物到 dist/（Tauri 配置 frontendDist 指向此处）
  build: {
    outDir: 'dist',
    target: 'esnext',
  },
})
