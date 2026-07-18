/**
 * useDesktop - 远程桌面 composable
 *
 * 封装 Canvas 渲染 + 帧事件监听 + 输入事件转发。
 */
import { ref, onUnmounted, type Ref } from 'vue'

import * as desktopService from '@/services/desktop'
import { listen } from '@/services/invoke'
import type { DesktopConfig } from '@/types/session'
import type { DesktopFrame, InputEvent } from '@/types/desktop'

export function useDesktop() {
  const sessionId = ref<string | null>(null)
  const error = ref<string | null>(null)
  const canvasRef = ref<HTMLCanvasElement | null>(null)
  const frameCount = ref(0)
  let unlistenFrame: (() => void) | null = null

  /** 建立 RDP/Host 连接 */
  async function connect(config: DesktopConfig) {
    // 监听帧
    unlistenFrame = await listen<DesktopFrame>('desktop_frame', (frame) => {
      if (frame.sessionId === sessionId.value) {
        renderFrame(frame)
        frameCount.value++
      }
    })

    try {
      sessionId.value = await desktopService.connect(config)
    } catch (e) {
      error.value = String(e)
      throw e
    }
  }

  /** 渲染一帧到 Canvas */
  function renderFrame(frame: DesktopFrame) {
    const canvas = canvasRef.value
    if (!canvas) return
    canvas.width = frame.width
    canvas.height = frame.height
    const ctx = canvas.getContext('2d')
    if (!ctx) return
    // 转 number[] 避免 SharedArrayBuffer 类型冲突
    const bytes: number[] =
      frame.rgba instanceof Uint8Array ? Array.from(frame.rgba) : frame.rgba
    // 用 createImageData + set 避免 ImageData 构造的类型问题
    const imgData = ctx.createImageData(frame.width, frame.height)
    imgData.data.set(bytes)
    ctx.putImageData(imgData, 0, 0)
  }

  /** 发送输入事件（坐标已反算为原始分辨率） */
  async function sendInput(event: InputEvent) {
    if (sessionId.value) {
      await desktopService.sendInput(sessionId.value, event).catch((e) => {
        error.value = String(e)
      })
    }
  }

  /** 断开连接 */
  async function disconnect() {
    if (sessionId.value) {
      await desktopService.disconnect(sessionId.value).catch(() => {})
      sessionId.value = null
    }
    unlistenFrame?.()
    unlistenFrame = null
  }

  onUnmounted(() => {
    disconnect()
  })

  return {
    sessionId,
    error,
    canvasRef,
    frameCount,
    connect,
    sendInput,
    disconnect,
  }
}
