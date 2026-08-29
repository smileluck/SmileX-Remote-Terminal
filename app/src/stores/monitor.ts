import { defineStore } from 'pinia'
import { ref, computed } from 'vue'

import type { MonitorSample, SessionHealth } from '@/types/monitor'
import * as monitorService from '@/services/monitor'

/** 每会话保留的采样点数（3s 间隔 × 60 ≈ 3 分钟曲线） */
const MAX_POINTS = 60

/**
 * 监控数据 store
 *
 * - 订阅 `monitor_metrics` 事件，按 sessionId 分发样本（滚动窗口）
 * - 轮询 `session_stats_all` 维护会话健康列表
 * - `activeSessionId` 为右栏当前监控目标（默认跟随激活的 SSH tab）
 */
export const useMonitorStore = defineStore('monitor', () => {
  /** sessionId → 滚动采样窗口（旧 → 新） */
  const samples = ref<Record<string, MonitorSample[]>>({})
  /** 会话健康列表（轮询） */
  const sessionHealths = ref<SessionHealth[]>([])
  /** 当前监控目标 sessionId（null = 未选择） */
  const activeSessionId = ref<string | null>(null)
  /** 已启动采样的 sessionId 集合 */
  const sampling = ref<Set<string>>(new Set())

  let started = false
  let pollTimer: number | null = null

  /** 当前目标的采样窗口 */
  const activeSamples = computed<MonitorSample[]>(
    () => (activeSessionId.value ? samples.value[activeSessionId.value] ?? [] : []),
  )
  /** 当前目标最新样本 */
  const latest = computed<MonitorSample | null>(() => activeSamples.value.at(-1) ?? null)

  /** 注入样本（事件回调） */
  function pushSample(s: MonitorSample) {
    const list = samples.value[s.session_id] ?? []
    list.push(s)
    if (list.length > MAX_POINTS) list.splice(0, list.length - MAX_POINTS)
    samples.value = { ...samples.value, [s.session_id]: list }
  }

  /** 启动某会话采样（幂等） */
  async function startSampling(sessionId: string) {
    if (sampling.value.has(sessionId)) return
    sampling.value = new Set(sampling.value).add(sessionId)
    try {
      await monitorService.start(sessionId)
    } catch (e) {
      sampling.value.delete(sessionId)
      sampling.value = new Set(sampling.value)
      console.warn('启动监控采样失败:', e)
    }
  }

  /** 停止采样并清理该会话数据 */
  async function stopSampling(sessionId: string) {
    if (!sampling.value.delete(sessionId)) return
    sampling.value = new Set(sampling.value)
    delete samples.value[sessionId]
    samples.value = { ...samples.value }
    if (activeSessionId.value === sessionId) activeSessionId.value = null
    monitorService.stop(sessionId).catch(() => {})
  }

  /** 初始化全局事件订阅 + 健康轮询（App 启动时调用一次） */
  async function init() {
    if (started) return
    started = true
    await monitorService.onMetrics(pushSample)
    const poll = async () => {
      try {
        sessionHealths.value = await monitorService.sessionStatsAll()
      } catch {
        /* 非 Tauri 环境忽略 */
      }
    }
    await poll()
    pollTimer = window.setInterval(poll, 3000)
  }

  /** 切换监控目标 */
  function setActive(sessionId: string | null) {
    activeSessionId.value = sessionId
    if (sessionId) startSampling(sessionId)
  }

  return {
    samples,
    sessionHealths,
    activeSessionId,
    activeSamples,
    latest,
    pushSample,
    startSampling,
    stopSampling,
    setActive,
    init,
  }
})
