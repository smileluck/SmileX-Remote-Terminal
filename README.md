# SmileX-Remote-Terminal

> 跨平台综合运维工具：SSH 终端 + 远程桌面 + AI 运维助手

SmileX-Remote-Terminal 是一款基于 **Tauri 2.x + Vue 3 + Rust** 构建的跨平台桌面应用，融合三大核心能力，为运维工程师、开发者和系统管理员提供统一、高效的远程操作体验。

## ✨ 核心特性

### 🔐 SSH 终端
- 基于 **russh 0.45** 纯 Rust 实现，无 libssh/openssl 依赖
- 支持密码 / 私钥文件 / 内存 PEM 私钥三种认证方式
- **PTY 背压机制**：有界 mpsc channel（容量 64）防止大输出 OOM
- **select! + 控制通道**：resize 指令优先处理，不被高频数据流饿死
- **Host Key 校验**：known_hosts 持久化 + 三种策略（Strict / AcceptNew / AcceptAll）
- xterm.js 终端渲染，支持 ANSI 转义、颜色、鼠标

### 🖥️ 远程桌面（架构就绪）
- **Windows/Linux**：RDP 协议（基于 ironrdp，接入中）
- **macOS**：自研被控 Host 协议，避免 VNC 卡顿
- Canvas 帧渲染 + 输入事件转发

### 🤖 AI 运维助手
- **多 LLM 配置档案管理**：同时保存多个 Provider 配置并快速切换激活
- 支持 **OpenAI 兼容**（OpenAI / DeepSeek / 智谱 / 通义千问等）/ **Anthropic Claude** / **本地 Ollama**
- **流式输出**：逐 token 推送，体验更佳
- **连通性测试**：保存前验证 API Key / Base URL 是否可用（10s 超时）
- Markdown 渲染 + 代码高亮
- Chat trait 已实现，Work trait 预留（运维自动化）

### 🗄️ 数据持久化与安全
- **SQLite + WAL 模式**：会话配置、known_hosts、LLLM 档案持久化
- **OS Keyring**：密码 / 私钥口令 / API Key 加密存储，非明文落盘
- **依赖反转**：ssh-core 定义 `KnownHostsStore` trait，desktop 反向实现注入

## 🏗️ 架构总览

```
┌─────────────────────────────────────────────────────────┐
│                    Vue 3 前端（app/）                     │
│  xterm.js + Canvas + Pinia + markdown-it + naive-ui     │
└───────────────────────┬─────────────────────────────────┘
                        │ Tauri IPC（invoke + emit）
┌───────────────────────▼─────────────────────────────────┐
│              Tauri 2.x 应用层（crates/desktop/）          │
│  Commands（SSH/桌面/AI/配置） + Events + Storage         │
└───┬───────────────────┬───────────────────┬─────────────┘
    │                   │                   │
┌───▼─────┐      ┌──────▼──────┐     ┌──────▼──────┐
│ ssh-core│      │remote-desktop│     │  ai-core    │
│         │      │    -core     │     │             │
│russh0.45│      │  ironrdp +   │     │ reqwest +   │
│         │      │  自研 Host   │     │ 多 Provider │
└─────────┘      └──────────────┘     └─────────────┘
```

### Monorepo 结构

```
SmileX-Remote-Terminal/
├── app/                          # Vue 3 前端
│   ├── src/
│   │   ├── components/           # UI 组件（终端/桌面/AI/设置/布局）
│   │   ├── composables/          # 组合式函数（useChat/useTerminal/useDesktop）
│   │   ├── services/             # Tauri IPC 封装
│   │   ├── stores/               # Pinia 状态管理
│   │   └── types/                # TypeScript 类型定义
│   └── package.json
├── crates/                       # Rust 领域核心
│   ├── ssh-core/                 # SSH 协议核心（russh 0.45）
│   │   ├── connection.rs         # 连接管理 + HostKeyHandler
│   │   ├── terminal.rs           # PTY 终端流 + select! 控制通道
│   │   ├── known_hosts.rs        # 主机密钥校验（KnownHostsStore trait）
│   │   └── keys.rs               # 密钥管理
│   ├── remote-desktop-core/      # 远程桌面核心（RDP + Host 协议）
│   ├── ai-core/                  # AI 核心抽象（LlmClient trait + Provider 实现）
│   │   └── provider/
│   │       ├── llm/              # OpenAI / Ollama 客户端实现
│   │       ├── chat.rs           # ChatProvider（对话历史 + 流式）
│   │       └── history.rs        # 多轮对话历史管理
│   └── desktop/                  # Tauri 应用入口
│       ├── src/
│       │   ├── commands/         # Tauri commands（session/ai/llm_profile/...）
│       │   ├── storage/          # SQLite + Keyring 持久化
│       │   └── events.rs         # 事件 payload 定义
│       └── tauri.conf.json
├── docs/                         # 设计文档
├── Cargo.toml                    # Rust workspace 根配置
└── package.json                  # pnpm workspace 根配置
```

## 📋 环境要求

### 必需依赖
- **Node.js** ≥ 20.x
- **pnpm** ≥ 8.x（推荐使用 [corepack](https://nodejs.org/api/corepack.html) 启用）
- **Rust** ≥ 1.75（stable 通道，推荐最新稳定版）
- **Tauri CLI 2.x**（通过 cargo 安装）

### 平台要求
- **Windows 10/11**：需要 WebView2 Runtime（通常已预装）
- **macOS**：需要 11.0 或更高版本
- **Linux**：需要 webkit2gtk 等系统依赖（见 [Tauri Prerequisites](https://v2.tauri.app/start/prerequisites/)）

## 🚀 快速开始

### 1. 克隆仓库

```bash
git clone https://github.com/smilex/SmileX-Remote-Terminal.git
cd SmileX-Remote-Terminal
```

### 2. 安装前端依赖

```bash
pnpm install
```

### 3. 安装 Tauri CLI（若未安装）

```bash
cargo install tauri-cli --version "^2.0"
```

### 4. 开发模式启动（推荐）

一键启动前端 dev server + Tauri 窗口，支持热更新：

```bash
# 方式一：通过根目录 package.json 脚本
pnpm tauri dev

# 方式二：直接调用 cargo tauri
cargo tauri dev
```

首次启动会编译全部 Rust crates，耗时较长（约 3-5 分钟）。后续启动增量编译，通常在 30 秒内。

### 5. 生产构建

打包为各平台原生安装包（Windows 为 `.msi` / `.exe`）：

```bash
pnpm tauri build
```

构建产物位于 `crates/desktop/target/release/bundle/`。

## 🛠️ 开发常用命令

### 前端

```bash
# 开发服务器（仅前端，无 Tauri 外壳）
pnpm dev

# 类型检查 + 生产构建
pnpm build

# 仅类型检查（不生成 dist）
pnpm --filter app type-check
```

### Rust

```bash
# 全 workspace 类型检查
cargo check --workspace

# 运行单元测试（ssh-core known_hosts 策略等）
cargo test --workspace

# 运行指定 crate 测试
cargo test -p ssh-core known_hosts
```

### 编译问题排查

若遇到 Rust 编译异常（如 ICE / 镜像缓存损坏），可尝试：

```powershell
# Windows PowerShell：关闭增量编译 + 使用独立 cargo 缓存
$env:CARGO_INCREMENTAL='0'
$env:CARGO_HOME='D:\cache\cargo-clean'
cargo check --workspace
```

> 项目根 `.cargo/config.toml` 已配置使用 crates.io 官方 sparse 协议，避开某些镜像缓存损坏问题。

## ⚙️ 首次使用指南

### 1. 配置 AI 助手
1. 启动应用后，点击右上角 **⚙ 设置** 按钮
2. 点击 **+** 新建 LLM 配置档案
3. 填写配置名称（如 "OpenAI 工作"）
4. 选择 Provider（OpenAI / Claude / Ollama）
5. 选择模型（如下拉选择 `gpt-4o`，或手动输入自定义模型名）
6. 填写 API Key（Ollama 无需）
7. 点击 **测试连接** 验证配置是否可用
8. **保存** 后点击 **设为默认** 激活该配置

### 2. 新建 SSH 会话
1. 左侧栏底部点击 **+ 新会话** 或顶部 **+ SSH**
2. 填写主机 / 端口 / 用户名 / 认证方式
3. 选择操作：
   - **仅连接**：临时连接，不保存配置
   - **保存并连接**：持久化到侧栏，下次一键连接
   - **仅保存**：保存但不立即连接
4. 首次连接未知主机时，根据策略可能需要确认 host key

### 3. 终端使用
- 输入命令即实时同步到远端
- 窗口尺寸变化自动触发 SSH `window-change` 请求
- 关闭 Tab 自动清理会话资源

## 🗃️ 数据存储位置

应用数据按平台规范存放：

| 平台 | 路径 |
|------|------|
| Windows | `%APPDATA%\com.smilex.remote-terminal\` |
| macOS | `~/Library/Application Support/com.smilex.remote-terminal/` |
| Linux | `~/.local/share/com.smilex.remote-terminal/` |

包含：
- `smilex-remote-terminal.db`：SQLite 数据库（会话配置 / known_hosts / LLM 档案）
- OS Keyring：密码 / 私钥口令 / API Key（加密存储）

## 🔐 安全模型

### 敏感数据分层存储

| 数据类型 | 存储位置 | 明文可见 |
|----------|----------|----------|
| 会话名称 / 主机 / 端口 / 用户名 | SQLite | ✅ |
| 会话密码 / 私钥口令 | OS Keyring | ❌（加密） |
| LLM 配置（Provider / Model / BaseURL） | SQLite | ✅ |
| LLM API Key | OS Keyring（`llm_profile:{id}:api_key`） | ❌（加密） |
| known_hosts 指纹 | SQLite | ✅ |

### Host Key 校验策略

| 策略 | 已知匹配 | 已知不匹配 | 未知 | 场景 |
|------|---------|------------|------|------|
| **Strict**（默认） | ✅ 接受 | ❌ 拒绝 | ❌ 拒绝 | 生产环境 |
| **AcceptNew** | ✅ 接受 | ❌ 拒绝 | ✅ 接受并保存 | 个人开发机 |
| **AcceptAll** | ✅ 接受 | ✅ 接受 | ✅ 接受 | CI/调试（危险） |

> 在连接表单中勾选「自动接受首次 host key」即启用 `AcceptNew` 策略。

## 📚 技术栈

### 前端
- **Vue 3.5** + Composition API + `<script setup>`
- **TypeScript 5.5** 严格类型
- **Vite 5.4** 构建
- **Pinia 2** 状态管理
- **xterm.js**（@xterm/xterm 5.5）终端渲染
- **naive-ui 2.39** UI 组件库
- **markdown-it** + **highlight.js** Markdown 渲染

### Rust 后端
- **Tauri 2.x** 跨平台桌面框架
- **tokio 1** 异步运行时
- **russh 0.45** 纯 Rust SSH 协议
- **rusqlite** SQLite 驱动
- **keyring** OS 凭据库访问
- **reqwest 0.12** HTTP 客户端（rustls TLS）
- **serde** / **serde_json** 序列化
- **tracing** 结构化日志

## 📖 文档

- [架构总纲](.trae/documents/smilex-remote-terminal-architecture.md)
- [设计文档](docs/design/)
  - [需求说明](docs/design/requirements.md)
  - [开发文档](docs/design/development.md)

## 🗺️ 路线图

### ✅ 已完成
- **阶段 2**：SSH 真实连接（russh 0.45 端到端实现）
- **阶段 3**：SQLite schema + 会话配置 CRUD + 新建会话 UI 闭环
- **阶段 4**：TerminalStream select! + 控制通道（resize 真实实现）+ known_hosts 持久化
- **阶段 5**：AI 设置页 + API Key 持久化
- **阶段 6**：多 LLM 配置档案管理 + 连通性测试 + Provider→Model 级联下拉

### 🚧 进行中 / 计划
- 真实 RDP 接入（ironrdp）
- macOS 自研 Host 协议被控端
- 远程桌面（rdp / host）会话配置 UI
- Host Key 校验接入 Tauri 弹窗确认
- AI Work trait 实现（运维自动化）
- 应用国际化（i18n）

## 📄 许可证

[MIT License](LICENSE)

## 🤝 贡献

欢迎提交 Issue 和 Pull Request。

- 提交前请运行 `cargo check --workspace` 和 `pnpm build` 确保通过
- 遵循现有代码风格（Rust: `cargo fmt`；Vue: 单文件组件 + `<script setup>`）
- 新增功能请补充对应的类型定义和文档注释

---

**SmileX-Remote-Terminal** — 让远程运维更简单、更智能。
