/**
 * Tauri invoke + event 封装
 *
 * 统一封装 Tauri IPC 调用，提供类型安全的接口。
 * 各命令组（session/desktop/ai）按模块拆分。
 */

// 检测 Tauri 环境（web 预览时回退到 noop）
const isTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window

/**
 * 安全调用 Tauri invoke
 * @throws 若不在 Tauri 环境，抛出友好错误
 */
export async function invoke<T = unknown>(
  cmd: string,
  args?: Record<string, unknown>,
): Promise<T> {
  if (!isTauri) {
    throw new Error(`非 Tauri 环境，无法调用 ${cmd}（请通过 cargo tauri dev 运行）`)
  }
  const { invoke: tauriInvoke } = await import('@tauri-apps/api/core')
  return tauriInvoke<T>(cmd, args)
}

/**
 * 监听 Tauri event（返回 unlisten 函数）
 */
export async function listen<T = unknown>(
  event: string,
  handler: (payload: T) => void,
): Promise<UnlistenFn> {
  if (!isTauri) {
    // 非 Tauri 环境返回 noop
    return () => {}
  }
  const { listen: tauriListen } = await import('@tauri-apps/api/event')
  return tauriListen<T>(event, (e) => handler(e.payload))
}

type UnlistenFn = () => void
