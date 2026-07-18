import { createApp } from 'vue'
import { createPinia } from 'pinia'
import { createRouter, createWebHashHistory } from 'vue-router'

import App from './App.vue'
import './styles/main.css'

// 应用入口：装配 Vue + Pinia + 路由
const app = createApp(App)

app.use(createPinia())

// 简单路由（MVP 阶段使用主视图承载多标签）
const router = createRouter({
  history: createWebHashHistory(),
  routes: [
    { path: '/', name: 'home', component: () => import('./views/HomeView.vue') },
  ],
})
app.use(router)

app.mount('#app')
