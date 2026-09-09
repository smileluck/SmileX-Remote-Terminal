/** crontab 环境错误类别（'none' = 正常） */
export type CrontabErrorKind = 'none' | 'not-installed' | 'no-permission' | 'unknown'

/**
 * 解析后的 crontab 行
 *
 * blank / comment / env 不参与列表展示，仅随整体回写原样保留；
 * job 行为可管理的任务条目。
 */
export interface CronLine {
  /** 在原始行数组中的下标（编辑/删除/停用定位用） */
  index: number
  kind: 'job' | 'env' | 'comment' | 'blank'
  /** 原始行内容（回写保真） */
  raw: string
  /** kind='job' 时解析出的 5 段表达式或 @reboot 等关键字 */
  schedule?: string
  /** kind='job' 时解析出的命令部分 */
  command?: string
  /** kind='job' 时有效；停用 = 行首加标记注释 */
  enabled?: boolean
}
