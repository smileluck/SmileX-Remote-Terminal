/**
 * 远程桌面类型定义
 */

/** 鼠标按键 */
export type MouseButton = 'left' | 'right' | 'middle'

/** 鼠标动作 */
export type MouseAction = 'down' | 'up' | 'move'

/** 修饰键状态 */
export interface Modifiers {
  ctrl: boolean
  shift: boolean
  alt: boolean
  meta: boolean
}

/** 输入事件（与 Rust 端 remote-desktop-core::input::InputEvent 对齐） */
export type InputEvent =
  | {
      kind: 'mouse'
      button: MouseButton
      action: MouseAction
      x: number
      y: number
      modifiers: Modifiers
    }
  | {
      kind: 'wheel'
      deltaX: number
      deltaY: number
      x: number
      y: number
      modifiers: Modifiers
    }
  | { kind: 'key_down'; code: string; modifiers: Modifiers }
  | { kind: 'key_up'; code: string; modifiers: Modifiers }
  | { kind: 'text_input'; text: string }
  | { kind: 'clipboard'; text: string }

/** 帧数据（来自 Tauri `desktop_frame` 事件） */
export interface DesktopFrame {
  sessionId: string
  seq: number
  width: number
  height: number
  rgba: number[] | Uint8Array
  keyFrame: boolean
}
