# detail.md 编辑器「@」task title 自动补全 popover（iter #280）

## Background

detail.md 已支持 ⌘K palette 弹完整 task picker / toolbar 「」按钮弹同 palette
但 mode 切到 insertRef。两条路径都需要中断 typing 流（鼠标 / 修饰键唤起）。
习惯了 Slack / GitHub / Notion 的 `@mention` 风格 owner 期望"键入 `@`
直接弹小 popover 实时过滤" — 最轻量、不破坏 typing 节奏。

本迭代加 `@`-trigger 内联 popover：键入 `@` 后 popover 弹起 → 继续打字
实时筛 → ↑↓ 选 → Enter / Tab 接受替换 `@query` 为 `「title」`。

## Changes

仅 `src/components/panel/PanelTasks.tsx`：

- **state**：
  - `atDismissedAt: number | null` — owner Esc 关 popover 时记下 @ 位置，
    cursor 仍在该 @ 词内时不重新弹（sticky dismiss）
  - `atSelectedIdx: number` — 当前选中 row

- **`atTrigger` useMemo**：从 (editingDetailContent, detailCursorPos) 派生
  `{ atPos, query } | null`：
  - 从 cursor 向回扫，找最近的 `@`
  - 仅 word-boundary @（前一字符为 start / whitespace）才算 trigger，避免
    email `foo@bar.com` 误触
  - 遇 whitespace 终止（说明 @ 在更早的 word 里）
  - dismissed-at-this-atPos 时返 null

- **`atSuggestions` useMemo**：filter visibleTasks by title contains query
  case-insensitive，cap 8

- **useEffect**：cursor 离开 @ 词后清 atDismissedAt（下次 owner 重新 @
  时仍可弹）

- **`acceptAtSuggestion(title)`**：把 `@query` 段替换成 `「title」`，cursor
  落 token 尾。复用 `「title」` ref token 协议（与 ⌘K palette /
  `🔗 拼为 ref` 同形）

- **`handleAtKeyDown(e)`**：popover 激活时拦截 ↑↓ / Enter / Tab / Esc，
  返 true 让调用方 early return

- **textarea onKeyDown hook**：edit 模式 textarea 在 onKeyDown 顶部加
  `if (handleAtKeyDown(e)) return;`，优先级最高 — 防 list-continue /
  bracket-pair / ⌘S 等下游 handler 抢键

- **popover render**：在 edit-mode textarea wrapper 内 absolute 定位
  `top: 100%, left: 8, right: 8`，浮在 textarea 底下；max-height 220 滚动；
  每条 row 显 title + Pn 灰字；hover sync 选中 idx；click 接受

## Key design decisions

- **仅 edit 模式触发**：split / preview 模式下 textarea 与 preview 同框，
  popover 浮在底下视觉混乱；keep it simple — 与既有"行号 gutter" 仅 edit
  模式 启用一致。
- **word-boundary @ 才触发**：避免 email / handle `foo@bar` 误触 owner
  正在打邮箱。前一字符为 start 或 whitespace 才算 word-start。
- **Sticky dismiss**：Esc 后 sticky 直到 cursor 离开 @ 词；owner 不必"删
  @ 再退出"或忍受连续弹起。
- **Tab / Enter 双接受键**：Slack / GitHub / IDE autocomplete 通行键位；
  二选一让 owner 不必背特定键。
- **cap 8 而非 30**：popover 高度由 cap 决定；8 行 ~180px 高，不挤占
  textarea 显示区。⌘K palette cap 30 是 explicit list 视图，本 inline
  picker 偏精准命中（owner 已打字过滤）。

## Verification

- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.20s)
