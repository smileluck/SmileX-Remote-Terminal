/**
 * 文件面板剪贴板（模块级单例，跨 FilePanel 实例共享）
 *
 * 复制/剪切只记录远端路径与模式，粘贴动作在目标面板执行：
 * - copy → 远端 `cp -a`（session_exec 静默通道）
 * - cut  → 远端 `mv`（比 SFTP rename 更稳，跨文件系统也可移动）
 * 跨会话粘贴暂不支持（复制跨主机需经本地中转，未实现）。
 */
import { ref } from 'vue'

export interface FileClipboardItem {
  sessionId: string
  /** 源绝对路径 */
  path: string
  /** 条目名（path 末段） */
  name: string
  isDir: boolean
  mode: 'copy' | 'cut'
}

export const fileClipboard = ref<FileClipboardItem | null>(null)
