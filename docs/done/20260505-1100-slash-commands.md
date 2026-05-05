# 输入框 `/` 快捷命令 — 开发计划

> 对应需求（来自 docs/TODO.md）：
> 输入框 `/` 快捷命令：聊天输入首字符 `/` 触发命令面板（/clear、/tasks、/search、/sleep N），减少切标签与重复 prompt。

## 目标

聊天输入框打头 `/` 时浮出命令面板（输入框上方），列出可用命令、随键入实时
过滤、键盘上下选 + Enter 执行，免去切标签或敲长 prompt。第一批命令：

| 命令 | 含义 |
| --- | --- |
| `/clear` | 清空当前会话的消息（仍保留 session id，不删除文件，只是 items / messages 清零） |
| `/tasks` | 切到「任务」标签 |
| `/search` | 在当前 panel 打开跨会话搜索面板 |
| `/sleep N` | 让 proactive mute N 分钟（`set_mute_minutes(N)`），N=0 解除 |
| `/help` | 在当前会话气泡里展示命令列表（不发给 LLM） |

## 非目标

- 不在桌面气泡的小输入框（`ChatPanel`）实现 —— 桌面气泡输入主要是即时聊天，
  长按 / 上下键选都不舒适；slash command 留在 panel 这个有空间的窗口里。
- 不让 LLM 见到 `/clear` 这种命令文本 —— 命令在前端拦截，不下行。
- 不做命令组合 / 管道 / 历史 —— Linux shell 风格在聊天面板里多余。
- 不写 README —— 体验补强。

## 设计

### UX

- 输入框首字符为 `/` → 浮出 `SlashCommandMenu` 在输入框正上方
- 菜单：每行 `<name>  <description>`，prefix-match 过滤当前 token
- 上 / 下 / Tab 切选；Enter 执行；Esc 关闭；点击行也执行
- 选中项高亮（背景 #e0f2fe + 蓝色字）
- 没有命令匹配 → 菜单展示"没有匹配的命令；输入 /help 查看全部"
- 命令带参数（`/sleep N`）→ 选中后**不立即执行**，而是回填到输入框等用户接着填参数；
  实际执行在用户按 Enter 时（与不带 `/` 的常规 send 路径同入口，但先识别 `/cmd args`）

### 命令拦截

`handleSubmit` 现在直接 `sendMessage`。改成：
1. 若 `input.startsWith("/")` → `parseSlashCommand(input)` →
   - Some(SlashAction) → 执行该 action（不调 sendMessage / LLM）
   - None / 未知 → 发一条本地 error 到 items 提示"未知命令，输入 /help 查看"
2. 否则正常 `sendMessage`

`SlashAction` enum：`Clear` / `OpenTasks` / `OpenSearch` / `Sleep(minutes)` /
`Help`（pure 测试好写）。

### 跨组件依赖

`PanelChat` → `/tasks` 需要让 `PanelApp` 切 tab。新加 prop `onRequestTab?:
(tab: "任务" | ...) => void`。`PanelApp` 把 `setActiveTab` 包一层传下去。

`/clear` 走原 `handleNewSession` 的"创建新空会话"语义？还是"原地清空当前 session"？
对齐用户预期：clear 应**清空当前** session，不创建新文件 —— 否则 session 列表会
膨胀。实现：reset items / messages → 走 saveCurrentSession（messages 只留 system）。

`/help` 在 items 里塞一条 `{ type: "assistant", content: <命令清单文案> }`，
不持久化（也可以持久化，作为本会话的一部分；选**不持久化**让 help 是临时的）。

### 文件改动

1. **新文件 `src/components/panel/slashCommands.ts`** —— 纯定义 + 解析：
   - `SlashCommand[]` 列表（name / description / parametric: bool）
   - `parseSlashCommand(input)` → `SlashAction | { kind: "unknown", name }`
   - `formatHelpText()` —— 命令清单的 Markdown-ish 文本
2. **新组件 `src/components/panel/SlashCommandMenu.tsx`** —— 浮窗 UI：
   - props: `query` (current input)、`active: boolean`、`onSelect(name)`、
     `onClose()`、`selectedIdx / setSelectedIdx`
   - 渲染过滤后的 commands 列表 + 键盘导航（已抬到父）
3. **`PanelChat.tsx` 接线**：
   - state `selectedSlashIdx: number`
   - input 的 onChange / onKeyDown：判断是否进入 slash 模式
   - handleSubmit 改为先尝试 parseSlashCommand
4. **`PanelApp.tsx`**：把 `setActiveTab` 通过新 prop 传给 `PanelChat`

### 测试

`slashCommands.ts` 是 pure；前端无测试基础设施，但本模块的边界条件（`/sleep`
带参数、未知命令、空尾随、大小写）值得**纯 TS 单测**——但项目无 vitest 配置。

我加少量"运行时自我检测"也不优雅。决定：**逻辑做到自验证**（每条 case 在用
法注释里写明输入 → 输出），手测 + tsc 即可。如果未来引入 vitest，这些函数已是
pure 形态可以直接测。

后端无变化（除了已有的 `set_mute_minutes` 命令）。

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | `slashCommands.ts` parse + 定义 |
| **M2** | `SlashCommandMenu.tsx` 浮窗 UI |
| **M3** | `PanelChat` 接线（state / onChange / onKeyDown / handleSubmit / handleSlashAction）+ `PanelApp` 传 onRequestTab |
| **M4** | tsc + build + TODO 清理 + done/ |

## 复用清单

- 现有 `searchMode` / `handleNewSession` 等 PanelChat 内部 API
- `set_mute_minutes` Tauri 命令（已注册）
- `saveCurrentSession`

## 待用户裁定的开放问题

- `/clear` 需要二次确认吗？聊天里清空可恢复（重启后或翻 session 历史也能找回，
  因为我们只清当前 session 的 items；session 文件还存在）。本轮**不做确认**——
  误操作恢复成本不高。
- `/sleep` 参数缺省 → 默认多少分钟？本轮选 `30`（与设置面板的滑块默认对齐）。
- 命令面板键盘快捷键：上 / 下 vs Vim h-j-k-l。本轮只支持上下 + Tab（可选）。

## 进度日志

- 2026-05-05 11:00 — 创建本文档；准备 M1。
- 2026-05-05 11:30 — 完成实现：
  - **M1**：新文件 `src/components/panel/slashCommands.ts` —— `SLASH_COMMANDS` 列表 + `parseSlashCommand` (纯) + `filterCommandsByPrefix` + `extractCommandPrefix` + `formatHelpText`。SlashAction 用 discriminated union（kind: clear/tasks/search/sleep/help/incomplete/unknown）；`/sleep` 默认 30 分钟，参数非整数走 unknown。
  - **M2**：新文件 `src/components/panel/SlashCommandMenu.tsx` —— 输入框上方浮窗（`position: absolute; bottom: 100%`），命令名 monospace + 描述行，选中项浅蓝背景 + 左边 2px 蓝条。`onMouseDown`（而非 click）确保 input blur 之前响应。selectedIdx 变化时 `scrollIntoView({ block: "nearest" })`。
  - **M3**：`PanelChat.tsx` 加 `selectedSlashIdx` 状态 + 派生 `slashPrefix` / `filteredCommands` / `slashMenuVisible`；`handleInputKeyDown` 接管 ↑↓/Tab/Esc/Enter（Enter 在已是完整命令时透传给 form，否则等价 Tab autocomplete —— 避免 `/cl` + Enter 触发"未知命令"）；`handleSubmit` 先尝试 `parseSlashCommand` 拦截。`executeSlash` 派发：clear → reset items / messages（保留 system soul）+ save_session；tasks → `onRequestTab("任务")`；search → 复用 searchMode；sleep → invoke `set_mute_minutes`；help → 在 items 推一条本地 assistant note。`pushLocalAssistantNote` 复用给所有"对话内 hint"反馈。`PanelApp.tsx` 把 `setActiveTab` 通过新 prop `onRequestTab` 传给 PanelChat。
  - **M4**：`pnpm tsc --noEmit` 干净；`pnpm build` 496 modules 全过（+2 新文件）。TODO 移除条目；本文件移入 `docs/done/`。
  - **README 不更新** —— 体验补强，与既有 panel 迭代同性质。
  - **设计取舍**：Enter 在 prefix 未完成时走 autocomplete 而非提交（避免误触"未知命令"反馈）；命令面板用 prefix-match 而非 fuzzy（命令量小，前缀直觉更准）；`/clear` 清当前 session 而非新建（避免 session 列表 churn）；`pushLocalAssistantNote` 走 setItems 而非 saveCurrentSession（help 等纯瞬时反馈不持久化，重启即清）。
  - **未做手动 dev 验证**：当前会话不便启动 Tauri 桌面 app；slash 解析层是 pure（输入 → 输出明确），UI 状态由 tsc + 既有 panel 模式保证。
