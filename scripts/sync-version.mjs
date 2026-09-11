// 从 tag（或裸版本号）同步版本到全部版本声明处
//
// 用法：
//   CI  —— node scripts/sync-version.mjs "$GITHUB_REF_NAME"（v0.1.5 → 0.1.5）
//   本地 —— pnpm version:bump 0.2.0（改完自行提交，再打同号 tag）
//
// 覆盖：crates/desktop/tauri.conf.json（决定安装包文件名）、
//       Cargo.toml（workspace 版本）、根与 app 的 package.json
import { readFileSync, writeFileSync } from 'node:fs'

const raw = process.argv[2]
if (!raw) {
  console.error('用法：node scripts/sync-version.mjs <版本号|v标签>')
  process.exit(1)
}

const version = raw.replace(/^refs\/tags\//, '').replace(/^v/, '')
if (!/^\d+\.\d+\.\d+(-[\w.]+)?$/.test(version)) {
  console.error(`非法版本号：${raw}（期望形如 0.2.0 或 v0.2.0）`)
  process.exit(1)
}

for (const file of ['crates/desktop/tauri.conf.json', 'package.json', 'app/package.json']) {
  const json = JSON.parse(readFileSync(file, 'utf8'))
  json.version = version
  writeFileSync(file, JSON.stringify(json, null, 2) + '\n')
}

const cargoPath = 'Cargo.toml'
const cargo = readFileSync(cargoPath, 'utf8')
const cargoVersionRe = /(\[workspace\.package\][\s\S]*?^version = ")[^"]*(")/m
if (!cargoVersionRe.test(cargo)) {
  console.error('未在 Cargo.toml 的 [workspace.package] 中找到 version 字段')
  process.exit(1)
}
writeFileSync(cargoPath, cargo.replace(cargoVersionRe, `$1${version}$2`))

console.log(`版本号已同步为 ${version}`)
