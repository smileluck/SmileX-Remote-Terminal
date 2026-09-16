# SmileX-Remote-Terminal 界面重构计划（8 项，每项一个 commit）

技术栈：Vue 3 + Pinia + Naive UI + Tauri 2，前端在 `app/src/`。实现顺序按 1→2→5→6→7→8→4→3（4 在 3 前：布局树是克隆落点的前提），每项完成即 commit。已确认的设计决策：监控/AI 按钮放终端右上工具组；文件面板修遮挡+可拖拽调宽；分屏克隆为独立新会话；会话 Tab 为展开/收起切换。

## 需求 1：新增入口统一到会话列表下拉（commit 1）

- `stores/ui.ts`：新增 `desktopConnectVisible` + `openDesktopConnectDialog()/closeDesktopConnectDialog()`
- 新组件 `components/common/DesktopConnectDialog.vue`：NModal 样式对齐 ConnectDialog；表单：类型（RDP / macOS 主机）、主机、端口（默认 3389）、用户名、密码；「连接」→ 构造 `DesktopConfig`（1920×1080/32 位默认）→ `tabs.addTab(kind, host)` + `updateTab(tab.id, { desktopConfig })` → 关弹窗
- `types/session.ts`：`TabItem` 增加 `desktopConfig?: DesktopConfig`
- `components/desktop/DesktopView.vue`：挂载时若 `tab.desktopConfig && !tab.sessionId` 自动连接（connecting 态）；失败回退显示内嵌表单（回填配置可重试）
- `components/layout/SideBar.vue`：header 的 Plus 按钮外包 `NDropdown`（选项：新建 SSH 会话 / 新建远程桌面连接）
- `components/layout/ActivityRail.vue`：移除「新建 SSH / 新建远程桌面 / AI 助手」三个按钮（设置保留）
- `components/layout/MainContent.vue`：欢迎页「远程桌面」按钮改开 DesktopConnectDialog
- `App.vue`：挂载 `<DesktopConnectDialog />`

## 需求 2：左栏只保留会话 Tab + 设置（commit 2）

- `ActivityRail.vue` 顶部新增「会话」tab 按钮（Terminal2 图标）：`active = !layout.sidebarCollapsed`，点击 `layout.toggleSidebar()`（展开侧栏到会话列表，已展开再点收起）

## 需求 5：折叠按钮移到左栏（commit 3）

- `ActivityRail.vue`：「会话」tab 上方加折叠/展开按钮（LayoutSidebarLeftCollapse，tooltip「折叠/展开会话栏（⌘B）」）→ `toggleSidebar()`
- `TopBar.vue`：移除折叠按钮

## 需求 6：监控按钮进终端工具组（commit 4）

- `TerminalView.vue` 的 `.view-tools`（终端右上竖排，文件管理按钮所在处）新增 ChartAreaLine 按钮：`active = layout.monitorVisible`，点击 `toggleMonitor()`（同 ⌘M），不依赖 `tab.sessionId`
- `TopBar.vue`：移除监控按钮（仅剩品牌标识）

## 需求 7：AI 助手开关进终端工具组（commit 5）

- `.view-tools` 新增 Robot 按钮：点击同 ⌘J 语义（右栏已在 agent 页签时收起，否则 `openRightPanel('agent')`）

## 需求 8：文件面板宽度修复（commit 6）

问题根因：`.view-tools` 绝对定位于 `.terminal-main` 右上角，文件面板（固定 240px）打开后被悬浮按钮组遮挡。
- `TerminalView.vue`：`.view-tools` 移入 `.split-area`（加 `position:relative`），按钮组落在终端区右上角、文件面板左侧
- `FilePanel.vue`：加左边缘拖拽手柄（复用 RightPanel 逻辑），宽度 200–480px
- `stores/layout.ts`：新增 `filesWidth`（默认 240）并纳入持久化

## 需求 4：分屏改布局树，按选中窗格分割（commit 7）

现状是扁平数组 + 全局单一方向，分割会重排所有窗格。重构为递归布局树：
```ts
type PaneNode  = { kind: 'pane'; id: string; sessionId: string | null }
type SplitNode = { kind: 'split'; dir: 'row'|'column'; children: LayoutNode[]; ratios: number[] }
```
- 新递归组件 `components/terminal/SplitLayout.vue`：pane 节点渲染 `PaneTerminal`（新增 `active` prop 高亮）；split 节点渲染 flex 容器 + 分割条（拖拽只改本节点 ratios，方向随节点）
- 分屏算法：在 `activePane` 位置将其替换为 `split(dir, [原窗格, 新窗格])`，其余布局不动；关闭窗格从树中移除，单子节点 split 自动折叠
- `activePaneId` 跟踪：PaneTerminal 根元素 mousedown 捕获 → emit focus；分屏按钮作用于选中窗格
- 顺带修复现有 bug：关闭窗格误断开其他 tab 的会话 → 用 `ownSessions: Set` 只断本 tab 克隆的会话
- 重连 watch：`tab.sessionId` 变化更新主窗格 p0

## 需求 3：分屏默认克隆当前连接为独立会话（commit 8）

- `services/session.ts`：`connect` 成功后记录 `configBySession: Map`；新增 `clone(sessionId, cols, rows)`（用原配置重新 connect，得到全新独立 SSH 连接 + PTY）；`disconnect` 清理映射。所有连接均流经 `sessionService.connect`（前端已持有完整配置），无需改 Rust 端
- `addPane(dir)`：选中窗格有 sessionId → loading 态 → `clone()` → 新窗格直接绑定；无 sessionId → 保持现有「绑定现有会话/新建连接」选择器
- 克隆会话记入 `ownSessions`，窗格关闭 / tab 卸载时断开 + stopSampling

## 验证与提交

- 每项 commit 前 `cd app && pnpm exec vue-tsc --noEmit` 类型检查；根目录 `pnpm tauri dev` 手动验证（注意 5173 端口）
- 8 个 commit（沿用现有 conventional + 中文风格）：
  1. `feat(ui): 会话列表新增按钮支持下拉新建SSH/远程桌面，统一弹窗入口`
  2. `feat(ui): 左侧活动栏改为会话Tab+设置`
  3. `feat(ui): 折叠侧边栏按钮移至左侧活动栏顶部`
  4. `feat(ui): 监控看板按钮移至终端工具组`
  5. `feat(ui): AI运维助手开关移至终端工具组`
  6. `fix(ui): 修复文件面板被工具组遮挡，支持拖拽调宽`
  7. `refactor(terminal): 分屏改为布局树，按当前选中窗格分割`
  8. `feat(terminal): 分屏默认克隆当前连接为独立会话`