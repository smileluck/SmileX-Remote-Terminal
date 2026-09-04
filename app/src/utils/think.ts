/**
 * 思考内容（<think>/<thinking> 标记）解析工具
 *
 * 推理模型（GLM/Qwen/DeepSeek-R1/Kimi thinking 等）常把思考过程以
 * <think>…</think> 标记混在正文里；后端也会把各协议的专用推理字段
 * （reasoning_content / thinking_delta 等）统一包装成该标记下发。
 * 展示层据此将思考过程折叠为「思考过程」块。
 */

export interface ThinkSegment {
  type: 'think' | 'text'
  text: string
  /** think 段是否已闭合（流式生成中末段可能未闭合） */
  closed: boolean
}

/** 兼容未闭合段：闭标记缺省时匹配到串尾（流式生成中） */
const THINK_RE = /<think(?:ing)?\s*>([\s\S]*?)(<\/think(?:ing)?\s*>|$)/g

/** 把内容拆成 think 段与正文段（保持顺序） */
export function splitThink(content: string): ThinkSegment[] {
  const out: ThinkSegment[] = []
  let last = 0
  for (const m of content.matchAll(THINK_RE)) {
    const idx = m.index ?? 0
    if (idx > last) out.push({ type: 'text', text: content.slice(last, idx), closed: true })
    out.push({ type: 'think', text: m[1], closed: m[2] !== '' })
    last = idx + m[0].length
  }
  if (last < content.length) out.push({ type: 'text', text: content.slice(last), closed: true })
  return out
}

/** 移除全部 think 段，仅保留正文（供命令块解析等场景） */
export function stripThink(content: string): string {
  return splitThink(content)
    .filter((s) => s.type === 'text')
    .map((s) => s.text)
    .join('')
}
