# 深色 / 浅色主题（迭代 4）— PanelChat 核心 surface 迁移

> 对应需求（来自 docs/TODO.md）：
> 把 PanelChat 的 inline color 迁到 var(--pet-color-*)；session list / form bar / message scroll 等核心 surface，保留 user/AI bubble、错误红、搜索高亮黄等语义色。

## 目标

迭代 1（token 系统 + PanelApp）/ 2（PanelTasks）/ 3（PanelDebug）已就位。
本轮把 PanelChat 的"框架级"surface 迁到 token：

- session 顶 bar（标题 / 搜索切换 / + 新会话 按钮）
- session 下拉（搜索面板 + 会话列表）
- 消息滚动区"开始聊天吧~" placeholder
- 输入栏（input + 发送按钮）
- assistant bubble 底色 + 文本（user bubble 走 accent token，统一"发送方=accent"）
- 复制按钮（默认 / hover / copied）
- 搜索结果行的 borderBottom 与 meta 文本

## 非目标 — 保留 motion 语义色

跨主题保持的"语义信号"色：

- **focus ring 蓝色 (`#38bdf8` / `rgba(56,189,248,0.18)`)** —— accent motion，
  与 outline 加固方案绑定
- **复制按钮 hover 浅蓝 (`#0ea5e9` / `#7dd3fc`)** —— accent + accent-light
- **selected session 浅蓝 tint (`#f0f9ff`)** —— "当前会话" 状态
- **search-mode active 蓝色 (`#0369a1` / `#e0f2fe`)** —— "搜索 mode 已开" 状态
- **delete 双态红 (`#fee2e2` / `#dc2626` / `#fff`)** —— danger arming
- **error bubble (`#fef2f2` / `#dc2626`)** —— 错误状态色
- **send 按钮 disabled 灰 `#cbd5e1`** —— 加载中禁用状态
- **search 高亮 mark (`#fef3c7` / `#92400e`)** —— 黄色高亮
- **copied 绿 `#16a34a`** —— 成功反馈
- **borderBottom `#f1f5f9`（dropdown 内分隔线，比 `#e2e8f0` 更轻）** —— 这条
  是个特例：与 framework `#e2e8f0` 区分语义（"列表内行间分隔" vs "panel 间
  分隔"）。保留浅灰 hex，dark 下需 iter5+ 单独 polish

## 设计

### 用 accent token 的语义色

`#0ea5e9`（发送 / user bubble / new-session-btn 文字）是 accent；用
`var(--pet-color-accent)` 让"主操作 / 主气泡 / 强调按钮"在 light/dark 下
都保持"primary action"色感（dark accent = `#38bdf8`，更亮更易读）。

### 迁移点表

| 区段 | from | to |
| --- | --- | --- |
| `loading` placeholder color | `#94a3b8` | muted |
| sessionBar bg / borderBottom | `#fff` / `#e2e8f0` | card / border |
| session 标题 color | `#1e293b` | fg |
| session 折叠箭头 color | `#94a3b8` | muted |
| newSessionBtnStyle border / bg / color | `#e2e8f0` / `#f8fafc` / `#0ea5e9` | border / bg / accent |
| 搜索 toggle inactive bg `#f8fafc` | (合并到 newSessionBtnStyle 默认 bg) | bg |
| 搜索 toggle inactive color `#475569` | hex | fg |
| 搜索输入 border / color | `#e2e8f0` / `#1e293b` | border / fg（+ 显式 bg=card） |
| 搜索清空 ✕ border / bg / color | `#e2e8f0` / `#fff` / `#64748b` | border / card / muted |
| 搜索 dropdown 提示 color | `#94a3b8` | muted |
| 会话列表无历史 color | `#94a3b8` | muted |
| 会话标题 color | `#1e293b` | fg |
| 会话 ts color | `#94a3b8` | muted |
| 消息空 placeholder color | `#94a3b8` | muted |
| 输入栏 borderTop / bg | `#e2e8f0` / `#fff` | border / card |
| input border / color（+ 显式 bg） | `#e2e8f0` / `#1e293b` | border / fg / card |
| send 按钮 active bg | `#0ea5e9` | accent |
| bubbleStyle assistant bg / color | `#fff` / `#1e293b` | card / fg |
| bubbleStyle user bg | `#0ea5e9` | accent |
| sessionBarStyle borderBottom / bg | `#e2e8f0` / `#fff` | border / card |
| sessionDropdownStyle borderBottom / bg | `#e2e8f0` / `#fff` | border / card |
| 复制按钮 border / bg / 默认 color | `#cbd5e1` / `#fff` / `#64748b` | border / card / muted |
| 复制按钮 hover CSS color / border-color | `#0ea5e9` / `#7dd3fc` | accent / 保持（无对应 token 的浅蓝） |
| 搜索结果 borderBottom | `#f1f5f9` | 保留（dropdown 内分隔线，hex 比 `--pet-color-border` 更轻） |
| 搜索结果文字 color | `#1e293b` | fg |
| 搜索结果 meta color | `#94a3b8` | muted |

### 测试

无单测；手测：
- light：与切换前完全一致（accent 仍是 #0ea5e9，看起来不变）
- dark：bar / bubble / input / dropdown 跟着切深；user bubble / 发送 / + 新会话
  按钮更亮（dark accent = #38bdf8）；focus ring / 选中 / 删除 / 错误 / 复制
  绿 / 高亮黄 / disabled 灰 都保持原色

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | 顶部 sessionBar / newSessionBtnStyle / focus CSS + copy CSS |
| **M2** | search 面板 + session 列表 dropdown |
| **M3** | message 区 placeholder + bubbleStyle |
| **M4** | input 栏 + send 按钮 |
| **M5** | 复制按钮 + 搜索结果 row |
| **M6** | tsc + build |

## 复用清单

- 既有 `src/theme.ts` token 系统 + CSS var
- 既有 PanelApp ☀️/🌙 toggle
- 模式与 iter2/3 一致：framework surface 用 token，motion 色保留

## 进度日志

- 2026-05-08 09:00 — 创建本文档；准备 M1。
- 2026-05-08 09:08 — M1 完成。loading placeholder muted、session bar 标题/箭头/搜索 toggle inactive 态（fg + bg）迁 token；copy hover CSS 颜色用 accent token；focus ring 与 copy hover 浅蓝边框（无对应 token）保留 hex。
- 2026-05-08 09:13 — M2 完成。search 输入框 + 清空 ✕ + 两条空状态 placeholder（border/card/fg/muted）+ 会话列表标题与 ts 迁 token；selected session #f0f9ff、search active #0369a1/#e0f2fe、删除两态红、borderBottom #f1f5f9 内行分隔线保留。
- 2026-05-08 09:16 — M3 完成。"开始聊天吧~" placeholder muted；bubbleStyle assistant bg/text → card/fg；user bubble bg → accent（dark 下变 #38bdf8 更亮的 primary 蓝）；user 文字 #fff 保持（on-accent）。
- 2026-05-08 09:18 — M4 完成。input 栏 borderTop/bg → border/card；input border/color/bg → border/fg/card；send 按钮 active bg → accent；isLoading disabled #cbd5e1 + 文字 #fff 保留（disabled motion）。
- 2026-05-08 09:21 — M5 完成。复制按钮三态（border/card/muted；copied 绿）+ 搜索结果行 fg/muted 文本迁 token；mark 高亮黄 #fef3c7/#92400e + borderBottom #f1f5f9 保留（motion + 内行分隔线）。
- 2026-05-08 09:24 — M6 完成。`pnpm tsc --noEmit` 0 错误；`pnpm build` 通过 (499 modules, 935ms)。归档至 done。
