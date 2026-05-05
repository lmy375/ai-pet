# 深色 / 浅色主题（迭代 7）— tinted section 加 dark 变体

> 对应需求（来自 docs/TODO.md）：
> tinted section 底色加 dark 变体（如 PanelDebug 紫/黄/绿/橙 5 段、PanelMemory butler 黄/蓝两段）— 用 `[data-theme="dark"]` CSS 覆盖到深色低饱和 tint，避免 dark 下亮色块刺眼。

## 目标

迭代 1-6 已用 token 把 framework surface 切到 `var(--pet-color-*)`，但 7 段
"section 类型"色块（紫/淡紫/黄/绿/橙 + butler 黄/蓝）仍 hardcoded
`#fdf4ff` / `#fefce8` / `#f0fdf4` / `#fff7ed` / `#faf5ff` / `#f0f9ff` 等浅色 hex。
切换到 dark 主题时这些亮色 tint 在 `#0f172a` 深底上格外刺眼，section 标题
（深色 hex）也读不清。

## 设计：扩展 token 系统加 6 对 tint

不走 `[data-theme="dark"]` CSS class —— 这要把所有 inline style 改成 className
+ stylesheet rule 才能让 specificity 胜出，工作量大且打破现有 inline-style
模式。

改用 token 一致路径：在 `src/theme.ts` 加 6 对 `tintX-bg` / `tintX-fg` 变量，
每对 light 值匹配现有 hex、dark 值用低饱和深色 + 反相文字色。inline style
的 `background: "#fdf4ff"` → `background: "var(--pet-tint-purple-bg)"`，主题
切换时 CSS 变量自动更新，无 React re-render。

### 6 对 tint 定义

| 变量族 | 用处 | light bg | light fg | dark bg | dark fg |
| --- | --- | --- | --- | --- | --- |
| **purple** | recentSpeeches | `#fdf4ff` | `#86198f` | `#251a32` | `#e879f9` |
| **lavender** | prompt-hints | `#faf5ff` | `#6b21a8` | `#221d33` | `#d8b4fe` |
| **yellow** | tool history / butler 每日小结 | `#fefce8` | `#854d0e` | `#2a2410` | `#fde68a` |
| **green** | feedback | `#f0fdf4` | `#065f46` | `#0c2419` | `#86efac` |
| **orange** | reminders | `#fff7ed` | `#9a3412` | `#2b1f10` | `#fdba74` |
| **blue** | butler 最近执行 | `#f0f9ff` | `#0369a1` | `#0c2236` | `#7dd3fc` |

dark bg 全部走"slate 主底 (`#0f172a`) 偏色微调"思路：保留色相但把 lightness 拉到
~10%，与主背景区分但不抢戏；dark fg 用对应色族的 100/200 阶（high lightness）
保对比度。

### 不在迭代 7 范围

- PanelDebug TOOL CALLS modal 子段（`#fffbeb` / `#fef3c7` 黄系子色）—— 嵌套在
  modal 内部、与本轮的"section bg"层级不同，dark 下视觉勉强能过，留 iter 8 polish
- 错误 banner（telegram / mcp `#fef2f2`）/ 高风险 review modal pink 内层
  / 警示 chip / mood-warning amber chip —— 都是"motion 状态色"，dark 下保持
  alarm 红 / amber，不归 section tint
- mood section / 数据卡片 等内部统计图配色 —— iter 8+ 单独 polish

## 实现

### 阶段 1 — `src/theme.ts`

`ThemeTokens` 加 12 个新键（每色 `xBg` / `xFg`）；`TOKENS.light` / `TOKENS.dark`
按上表填值；`applyTheme` 现在 for-of `Object.entries(tokens)` 自动写出全部
变量（CSS 变量名遵循已有 `--pet-color-*` 前缀，加新前缀 `--pet-tint-*`）。

为了让 prefix 区分，不在 `--pet-color-*` 命名空间下塞 tint，单独建
`--pet-tint-{purple,lavender,yellow,green,orange,blue}-{bg,fg}` 共 12 变量。
`applyTheme` 拓展为同时写两套 prefix。

### 阶段 2 — PanelDebug 5 段迁移

| 段 | 改动 |
| --- | --- |
| recentSpeeches | bg `#fdf4ff` → `var(--pet-tint-purple-bg)`；header `#86198f` → `var(--pet-tint-purple-fg)` |
| 工具调用历史 | bg `#fefce8` → `var(--pet-tint-yellow-bg)`；header / "暂无工具" / 空过滤都用 `var(--pet-tint-yellow-fg)` |
| 反馈记录 | bg `#f0fdf4` → green-bg；header / 空状态 / ts 用 green-fg |
| 提醒事项 | bg `#fff7ed` → orange-bg；header `#9a3412` → orange-fg |
| prompt-hints | bg `#faf5ff` → lavender-bg；标题段 `#6b21a8` → lavender-fg |

### 阶段 3 — PanelMemory butler 2 段迁移

| 段 | 改动 |
| --- | --- |
| 每日小结 | bg `#fefce8` → yellow-bg；border `#fde68a` 保留（黄段标识）；header `#a16207` → yellow-fg；ts `#a16207` → yellow-fg；body `#374151` → fg（独立于 tint） |
| 最近执行 | bg `#f0f9ff` → blue-bg；border `#bae6fd` 保留；header `#0369a1` → blue-fg；body `#475569` → fg；desc `#64748b` → muted；ts `#94a3b8` → muted |

体内 motion 文字（delete `#dc2626` / update `#0d9488` / 黄/蓝标志色）保留。

### 测试

无单测；手测：
- light 模式：与切换前完全一致（tint 值精确匹配旧 hex）
- 切 dark：5 + 2 段 tinted bg 变深色低饱和、heading 变 light 高饱和；body 文字 + ts 变 light 跟着切；其它 motion 段 / chip / banner 全部保留原色

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | theme.ts + applyTheme 写新 12 变量 |
| **M2** | PanelDebug 5 段 bg + header |
| **M3** | PanelMemory 2 段 bg + header + body |
| **M4** | tsc + build + 手测 light/dark |

## 复用清单

- 既有 `src/theme.ts` token 基础结构
- 既有 6 个 framework token（继续保持向前兼容）

## 进度日志

- 2026-05-08 12:00 — 创建本文档；准备 M1。
- 2026-05-08 12:08 — M1 完成。`src/theme.ts` 加 `ThemeTints` interface + `TINTS` 表 light/dark 各 12 值；`applyTheme` 同时写 `--pet-color-*`（旧）+ `--pet-tint-*-{bg,fg}`（新 12 个）；驼峰转 kebab `purpleBg → purple-bg` 由内部 `camelToKebab` 完成。
- 2026-05-08 12:14 — M2 完成。PanelDebug 5 段：prompt-hints lavender / recentSpeeches purple / 工具历史 yellow / 反馈记录 green（含 ts 单元格） / 提醒事项 orange — bg + section header 全用 tint var；NATURE_META 色 / kindColor / motion chip 全保留。
- 2026-05-08 12:18 — M3 完成。PanelMemory butler 2 段：每日小结 yellow（bg + header + ts → tint，body → fg） / 最近执行 blue（bg + header → tint，body → fg，desc/ts → muted）；border #fde68a / #bae6fd / actionColor delete-red / update-teal 全部保留。
- 2026-05-08 12:22 — M4 完成。`pnpm tsc --noEmit` 0 错误；`pnpm build` 通过 (499 modules, 951ms)。归档至 done。
