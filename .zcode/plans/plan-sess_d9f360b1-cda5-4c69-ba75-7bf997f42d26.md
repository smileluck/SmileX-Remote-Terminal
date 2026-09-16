# LLM 支持国内厂商(API / Coding Plan 接入方式区分)

## 背景

当前 `LlmProvider` 仅 `openai/claude/ollama` 三值,claude 是 OpenAI 客户端占位,无 Anthropic 原生协议。国内厂商的 Coding Plan 订阅统一走 **Anthropic 兼容端点**(智谱 `open.bigmodel.cn/api/anthropic`、Kimi `api.moonshot.ai/anthropic`、DeepSeek `api.deepseek.com/anthropic`、MiniMax `api.minimaxi.com/anthropic`、百炼 `/apps/anthropic`),按量 API 走 OpenAI 兼容端点,且两种方式的 Key 常不通用(Kimi 订阅 Key 在 platform.kimi.ai 签发,按量 Key 在 moonshot 开放平台)。

核心设计:把「厂商预设」(决定 UI/默认端点/模型/hint)与「协议」(决定用哪个 HTTP 客户端)分离,中间用「接入方式 auth_mode」(`api` 按量 / `coding_plan` 订阅)桥接:同一厂商选不同接入方式 → 自动切 Base URL、协议、模型列表。

## 1. ai-core:Anthropic 客户端 + 协议分离

`crates/ai-core/src/provider/llm/mod.rs`:
- 新增 `LlmProtocol { OpenAiCompatible, Anthropic, Ollama }`(serde snake_case,含 `Copy`)
- `LlmProvider` 扩展 `Zhipu, DeepSeek, Moonshot, Qwen, Minimax`(serde lowercase,与现有风格一致)
- 新增 `LlmAuthMode { Api, CodingPlan }`(serde snake_case → `"api"`/`"coding_plan"`)
- `resolve_protocol(provider, auth_mode)`:claude→Anthropic;ollama→Ollama;国内厂商+coding_plan→Anthropic,否则→OpenAiCompatible
- `create_client` 改按 resolve_protocol 分发,删除 claude 的 OpenAI 占位

`crates/ai-core/src/provider/mod.rs` 的 `LlmProviderConfig`(已有 camelCase rename_all)加:
```rust
#[serde(default)]
pub auth_mode: Option<LlmAuthMode>,
```
(遵循项目 serde 约定:enum snake_case、struct camelCase,防 `#[serde(default)]` 静默吞字段)

新文件 `crates/ai-core/src/provider/llm/anthropic.rs`(仿 openai.rs 手写 reqwest + SSE):
- `POST {base}/v1/messages`,默认 base `https://api.anthropic.com`
- Headers:`x-api-key` + `anthropic-version: 2023-06-01`;遇 401 用 `Authorization: Bearer` 重试一次(兼容 coding plan 端点的 ANTHROPIC_AUTH_TOKEN 约定,官方 Anthropic 不允许同时带两种头,故用重试而非叠加)
- Body:`model`、`max_tokens: 4096`(Anthropic 必填)、`stream: true`、`system`(从 messages 抽出 System 角色放顶层,剩余 user/assistant 相邻同角色合并)、`messages`
- SSE 解析:沿用按行缓冲 + `data:` 提取模式,取 `type=="content_block_delta"` 的 `delta.text`;`message_stop` 结束
- 抽出纯函数(`split_system`、delta 提取)便于单元测试

## 2. SQLite:llm_profiles 加 auth_mode 列

`crates/desktop/src/storage/sqlite.rs`:
- `LlmProfile` struct 加 `#[serde(default)] pub auth_mode: Option<String>`
- `init_schema`:CREATE TABLE 语句加 `auth_mode TEXT`(新库);并新增幂等迁移——`PRAGMA table_info(llm_profiles)` 检查列不存在时 `ALTER TABLE llm_profiles ADD COLUMN auth_mode TEXT`(旧库)
- `save_llm_profile` / `list_llm_profiles` / `get_llm_profile` / `get_active_llm_profile` 的 SQL 与 `row_to_llm_profile` 全部补列
- 单测:先用旧 schema(无该列)建库再 open,验证迁移后可读写 auth_mode

## 3. desktop 命令层

`crates/desktop/src/commands/llm_profile.rs`:
- `parse_provider` 增加 5 个新预设值匹配
- `build_config` 解析并透传 `auth_mode`(String→Option<LlmAuthMode>)
- `migrate_legacy_config` 构造 profile 时补 `auth_mode: None`
- 其余命令(test/set_active/load)经 build_config 自动生效,无需改

## 4. 前端类型与预设(app/src/types/settings.ts)

- `LlmProvider` 联合类型加 `'zhipu' | 'deepseek' | 'moonshot' | 'qwen' | 'minimax'`
- 新增 `LlmAuthMode = 'api' | 'coding_plan'`;`LlmProfile`/`LlmProviderConfig` 加 `authMode?: string | null`
- `ProviderOption` 重构:内含 `accessModes: Array<{ value, label, defaultBaseUrl, models, hint }>`;现有 3 项为单模式,国内 5 厂商为双模式
- 新增 `findAccessMode(provider, authMode)`(缺失回退首模式);`createDefaultProfileFields` 适配(默认 'api',baseUrl/model 取该模式预设)

预设(实现时按官方文档核对最新模型名与确切端点;用户已确认全部 5 家):

| 预设 | api(按量,OpenAI 协议) | coding_plan(Anthropic 协议) |
|---|---|---|
| 智谱 GLM | `https://open.bigmodel.cn/api/paas/v4` | `https://open.bigmodel.cn/api/anthropic` |
| DeepSeek | `https://api.deepseek.com/v1` | `https://api.deepseek.com/anthropic`(同 Key 按量,label 注明「Anthropic 兼容」) |
| Kimi | `https://api.moonshot.cn/v1` | `https://api.moonshot.ai/anthropic`(hint 强调独立订阅、Key 与按量不通用) |
| 通义千问 | `https://dashscope.aliyuncs.com/compatible-mode/v1` | 百炼 Anthropic 兼容端点(`/apps/anthropic` 结尾,核对确切 URL) |
| MiniMax | `https://api.minimaxi.com/v1` | `https://api.minimaxi.com/anthropic` |

选择国内预设时 baseUrl 自动填具体厂商端点(落库为具体值,后端无需映射表);openai/claude 保持「留空=官方默认」。

## 5. 设置页 UI(SettingsView.vue)

- Provider 下拉用 NSelect 分组:「通用」(OpenAI/Claude/Ollama)+「国内厂商」(5 项)
- 新增「接入方式」下拉,仅当预设有 2 种模式时显示;切换时自动重置 baseUrl 为该模式默认、model 为首个预设,并更新 hint(用户手改过的 baseUrl 允许被覆盖——切换即重置,行为简单可预期)
- 档案列表 meta 与表单显示接入方式标签(「API」/「Coding Plan」小 tag)
- hint 文案写清 Key 签发平台差异(尤其 Kimi)

## 6. 验证

- `cargo test -p ai-core -p smilex-desktop`(Anthropic 解析单测 + sqlite 迁移单测)
- `pnpm typecheck` + `pnpm build`(app)
- 手动 `pnpm tauri dev`:新建各厂商×接入方式档案,确认自动填充与保存回显;真实 Key 连通性由用户用「测试连接」验证

## 7. 提交拆分(逐项 commit,conventional 前缀 + 中文正文)

1. `feat(ai-core): 新增 Anthropic 协议客户端,Provider 扩展国内厂商与接入方式`
2. `feat(desktop): llm_profiles 增加 auth_mode 列迁移并透传命令链路`
3. `feat(ui): 设置页预置国内厂商,支持 API/Coding Plan 接入方式切换`

实现期间注意:文件可能被并行 commit 改动(Edit 报 modified 时重新 Read);编辑前先 `git log` 核对。