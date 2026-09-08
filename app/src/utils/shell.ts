/** shell 双引号转义：用于拼接 cd 等命令 */
export function shellQuote(s: string): string {
  return '"' + s.replace(/(["\\$`])/g, '\\$1') + '"'
}
