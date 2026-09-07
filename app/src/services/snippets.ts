/**
 * 命令片段 & 历史命令封装
 *
 * 对接后端 `snippet_*` / `history_*` commands。
 */

import { invoke } from './invoke'

/** 片段类型：command = 命令；service = 服务（command 字段存服务名） */
export type SnippetKind = 'command' | 'service'

/** 命令片段 */
export interface CommandSnippet {
  id: string
  name: string
  command: string
  tags: string
  /** 所属分组名（空串 = 未分组） */
  groupName: string
  /** 排序序号（分组内 + 分组间共用，越小越靠前） */
  sortOrder: number
  /** 条目类型（默认 'command'） */
  kind: SnippetKind
  /** 服务条目的自定义状态检查命令（空串 = 默认 systemctl is-active） */
  checkCmd: string
  createdAt: number
}

/** 命令历史条目 */
export interface CommandHistory {
  id: number
  sessionId: string
  command: string
  createdAt: number
}

export function snippetList(): Promise<CommandSnippet[]> {
  return invoke<CommandSnippet[]>('snippet_list')
}

export function snippetSave(snippet: CommandSnippet): Promise<void> {
  return invoke<void>('snippet_save', { snippet })
}

export function snippetDelete(id: string): Promise<boolean> {
  return invoke<boolean>('snippet_delete', { id })
}

/** 批量重排命令片段（分组/排序拖拽落库） */
export function snippetReorder(snippets: CommandSnippet[]): Promise<void> {
  return invoke<void>('snippet_reorder', { snippets })
}

export function historyAdd(sessionId: string, command: string): Promise<void> {
  return invoke<void>('history_add', { sessionId, command })
}

export function historyList(limit = 100): Promise<CommandHistory[]> {
  return invoke<CommandHistory[]>('history_list', { limit })
}

export function historyClear(): Promise<void> {
  return invoke<void>('history_clear')
}
