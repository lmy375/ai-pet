# proactive prompt 上一轮历史预览 — 开发计划

> 对应需求（来自 docs/TODO.md）：
> proactive prompt 上一轮历史预览：PanelDebug 顶部"上次主动开口 prompt"折叠后看不到内容；展开时太长。加 "上次主动 prompt（{N} 字符 / {N} 行）"折叠头，让用户一眼判断是否值得展开。

## 目标

PanelDebug 的 "看上次 prompt" modal 把 PROMPT / REPLY 两段长文本一并铺开。
典型 prompt 5-10k 字符，reply 1-2k 字符；想跨 turn 翻看时垂直空间被吃掉。
本轮给两段加可折叠 header（含字符 + 行数 hint），用户能一眼判断要不要展开
具体内容。

## 非目标

- 不动 TOOL CALLS section —— 那里每条 tool call 已经是 per-row 折叠，再
  套一层"全部折叠"反而手感怪。
- 不持久化折叠态 —— 关掉 modal 重开应该回到默认（与既有 turnIndex 复位
  同语义；折叠态是临时阅读姿态，不该携带）。
- 默认仍展开，不强制收 —— 现有用户开 modal 已经习惯一眼看全文；强制收
  会让"看上次 prompt"按钮含义变成"看 metadata"，错位。
- 不动顶部 chip strip 的 `prompt {N} / reply {N} chars` —— 那是横向压力
  指示，与 section 内的 metadata 互补不冲突。

## 设计

### state

`promptCollapsed: boolean` 和 `replyCollapsed: boolean`，默认 false。
modal 关闭（`showLastPrompt` 切回 false）/ turn 切换不重置 —— 用户在多
turn 间切看时，折叠偏好可保留这一会话。

### header 文案

PROMPT header 加：
- 折叠箭头 ▾ / ▸（与既有 tool call 风格一致）
- metadata：`{N} 字符 · {N} 行`（行数 = `lastPrompt.split("\n").length`，
  空字符串 → 0 行而非 1 行 —— "0 行" 更贴近"啥也没有"的视觉直觉）

整个 header 容器 cursor: pointer，点击切换；保持复制按钮 stopPropagation
不影响折叠（用户点复制不应关 section）。

### REPLY 同理

文案配色保持既有绿底（`#f0fdf4` / `#166534`），只加箭头 + 字数行数。

### 折叠时 body 不渲染

简单 `{!promptCollapsed && <pre>...</pre>}` 即可；无需保留布局空间。
section 之间的 borderBottom 由 header 容器自带，不会因 body 消失出现
"双 border" 视觉粘连。

### 纯函数

`countLines(text: string): number` 放在 PanelDebug 顶部；空字符串 → 0
行。让 caller 不必在 JSX 内联 ternary。

## 测试

`PanelDebug` 是 IO 重容器，前端无 vitest；靠 tsc + 手测足以。countLines
内联于 React 组件作用域，不暴露到模块外。

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | promptCollapsed / replyCollapsed state + countLines |
| **M2** | PROMPT section header 改造（箭头 + meta + click） |
| **M3** | REPLY section header 同款改造 |
| **M4** | tsc + build + cleanup |

## 复用清单

- 既有 ▾/▸ 折叠箭头风格（tool call section）
- 既有 section header 容器布局
- 既有 stopPropagation 在按钮上的模式

## 进度日志

- 2026-05-06 22:00 — 创建本文档；准备 M1。
- 2026-05-06 22:05 — M1 完成。`promptCollapsed` / `replyCollapsed` state 默认 false 保留首次打开"一眼看全文"既有 UX；`countLines(text)` helper 空字符串返 0（贴近"啥也没有"直觉）。
- 2026-05-06 22:15 — M2 完成。PROMPT header 加点击折叠：▾/▸ 箭头 + `{N} 字符 · {N} 行` metadata；复制按钮 stopPropagation 防误折；body `<pre>` 改成 `!collapsed && ...`。
- 2026-05-06 22:20 — M3 完成。REPLY header 同款改造（绿底配色保持）；按钮 stopPropagation 同处理。
- 2026-05-06 22:25 — M4 完成。`pnpm tsc --noEmit` 0 错误；`pnpm build` 通过 (498 modules, 936ms)。归档至 done。
